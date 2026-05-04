//! Agent lifecycle management
//!
//! An agent is the fundamental execution unit in TuniCore.
//! Unlike Unix processes, agents are:
//! - Capability-gated from birth (zero privileges by default)
//! - Budget-limited (max memory, compute ticks, I/O ops)
//! - Time-limited (max lifetime, auto-killed on expiry)
//! - Audited (every syscall logged)

use crate::cap_table::{AgentId, CapHandle};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Next agent ID generator
static NEXT_AGENT_ID: AtomicU32 = AtomicU32::new(1);

/// Maximum number of concurrent agents
const MAX_AGENTS: usize = 256;

/// Agent execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// Being initialized, loading code
    Initializing,
    /// Running and consuming resources
    Active,
    /// Waiting for I/O, compute result, or channel message
    Blocked,
    /// Paused (can be resumed, resources held)
    Suspended,
    /// Terminated (cleanup pending, caps being revoked)
    Terminated,
}

/// Hard limits on what an agent can consume
#[derive(Debug, Clone)]
pub struct ResourceBudget {
    /// Maximum heap bytes this agent may allocate
    pub memory_bytes: u64,
    /// Maximum CPU ticks this agent may use
    pub compute_ticks: u64,
    /// Maximum I/O operations this agent may perform
    pub io_ops: u64,
    /// Maximum number of sub-agents this agent may spawn
    pub max_children: u16,
    /// Currently used memory
    pub used_memory: u64,
    /// Currently used compute ticks
    pub used_compute: u64,
    /// Currently used I/O ops
    pub used_io: u64,
    /// Current number of children
    pub current_children: u16,
}

impl ResourceBudget {
    /// Create a default budget
    pub fn default_budget() -> Self {
        Self {
            memory_bytes: 16 * 1024 * 1024, // 16 MiB
            compute_ticks: 1_000_000,         // ~10 seconds
            io_ops: 10_000,
            max_children: 4,
            used_memory: 0,
            used_compute: 0,
            used_io: 0,
            current_children: 0,
        }
    }

    /// Check if memory allocation is within budget
    pub fn can_allocate(&self, bytes: u64) -> bool {
        self.used_memory + bytes <= self.memory_bytes
    }

    /// Check if an I/O operation is within budget
    pub fn can_io(&self) -> bool {
        self.used_io < self.io_ops
    }

    /// Check if a child spawn is within budget
    pub fn can_spawn_child(&self) -> bool {
        self.current_children < self.max_children
    }

    /// Record memory usage
    pub fn record_memory(&mut self, bytes: u64) {
        self.used_memory += bytes;
    }

    /// Record I/O usage
    pub fn record_io(&mut self) {
        self.used_io += 1;
    }

    /// Record compute usage
    pub fn record_compute(&mut self, ticks: u64) {
        self.used_compute += ticks;
    }
}

/// An agent running in TuniCore
pub struct Agent {
    /// Unique agent ID
    pub id: AgentId,
    /// Human-readable name (for audit logs)
    pub name: [u8; 64],
    /// Name length
    pub name_len: usize,
    /// Current execution state
    pub state: AgentState,
    /// All capability handles owned by this agent
    pub capabilities: Vec<CapHandle>,
    /// Resource budget (hard limits + usage tracking)
    pub budget: ResourceBudget,
    /// Parent agent (who spawned this agent)
    pub parent: Option<AgentId>,
    /// Tick when this agent was spawned
    pub spawn_tick: u64,
    /// Maximum lifetime in ticks (0 = unlimited)
    pub max_lifetime: u64,
}

impl Agent {
    /// Create a new agent
    pub fn new(
        name: &str,
        parent: Option<AgentId>,
        budget: ResourceBudget,
        max_lifetime: u64,
        current_tick: u64,
    ) -> Self {
        let id = AgentId(NEXT_AGENT_ID.fetch_add(1, Ordering::Relaxed));

        let mut name_buf = [0u8; 64];
        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(63);
        name_buf[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

        Agent {
            id,
            name: name_buf,
            name_len: copy_len,
            state: AgentState::Initializing,
            capabilities: Vec::new(),
            budget,
            parent,
            spawn_tick: current_tick,
            max_lifetime,
        }
    }

    /// Get agent name as str
    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("?")
    }

    /// Check if this agent has exceeded its lifetime
    pub fn is_expired(&self, current_tick: u64) -> bool {
        self.max_lifetime > 0 && current_tick >= self.spawn_tick + self.max_lifetime
    }

    /// Check if this agent has exceeded its compute budget
    pub fn is_over_budget(&self) -> bool {
        self.budget.used_compute >= self.budget.compute_ticks
    }

    /// Add a capability to this agent's set
    pub fn add_capability(&mut self, handle: CapHandle) {
        self.capabilities.push(handle);
    }

    /// Transition to a new state
    pub fn set_state(&mut self, new_state: AgentState) {
        self.state = new_state;
    }
}

/// Agent table — manages all active agents
pub struct AgentTable {
    agents: Vec<Agent>,
}

impl AgentTable {
    pub const fn new() -> Self {
        Self {
            agents: Vec::new(),
        }
    }

    /// Spawn a new agent
    pub fn spawn(
        &mut self,
        name: &str,
        parent: Option<AgentId>,
        budget: ResourceBudget,
        max_lifetime: u64,
        current_tick: u64,
    ) -> Result<AgentId, &'static str> {
        if self.agents.len() >= MAX_AGENTS {
            return Err("max agents reached");
        }

        let agent = Agent::new(name, parent, budget, max_lifetime, current_tick);
        let id = agent.id;
        self.agents.push(agent);
        Ok(id)
    }

    /// Find an agent by ID
    pub fn get(&self, id: AgentId) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }

    /// Find an agent by ID (mutable)
    pub fn get_mut(&mut self, id: AgentId) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|a| a.id == id)
    }

    /// Kill an agent and mark it for cleanup
    pub fn kill(&mut self, id: AgentId) -> Result<(), &'static str> {
        let agent = self.get_mut(id).ok_or("agent not found")?;
        agent.set_state(AgentState::Terminated);

        // Revoke all capabilities in cap_table
        crate::cap_table::CAP_TABLE.lock().revoke_all_for_agent(id);

        Ok(())
    }

    /// Get number of active (non-terminated) agents
    pub fn active_count(&self) -> usize {
        self.agents
            .iter()
            .filter(|a| a.state != AgentState::Terminated)
            .count()
    }

    /// Garbage collect: remove terminated agents
    pub fn gc(&mut self) {
        self.agents.retain(|a| a.state != AgentState::Terminated);
    }

    /// Check all agents for expired lifetimes and kill them
    pub fn enforce_timeouts(&mut self, current_tick: u64) {
        let expired_ids: Vec<AgentId> = self
            .agents
            .iter()
            .filter(|a| a.is_expired(current_tick) && a.state != AgentState::Terminated)
            .map(|a| a.id)
            .collect();

        for id in expired_ids {
            let _ = self.kill(id);
        }
    }
}

/// Global agent table
use spin::Mutex;
pub static AGENT_TABLE: Mutex<AgentTable> = Mutex::new(AgentTable::new());
