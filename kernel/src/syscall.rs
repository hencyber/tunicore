//! Syscall gate - the enforcement layer
//!
//! ALL agent operations go through this gate. No capability = no access.
//! This is "the guard" made concrete in code.
//!
//! Flow: Agent → syscall() → cap check → execute → audit → return

use crate::audit::{self, AuditEvent, AUDIT_LOG};
use crate::cap_table::{self, AgentId, CapError, CapHandle, CAP_TABLE};
use crate::capability::types::Rights;
use crate::interrupts;
use crate::resource::ResourceRef;
use crate::serial_println;

/// Syscall identifiers
#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum SyscallId {
    // ─── Capability management ───
    /// Request a new capability (kernel grants only)
    CapRequest = 0x01,
    /// Delegate capability to another agent
    CapDelegate = 0x02,
    /// Revoke a capability
    CapRevoke = 0x03,
    /// Query capability status
    CapQuery = 0x04,

    // ─── Agent lifecycle ───
    /// Spawn a sub-agent
    AgentSpawn = 0x10,
    /// Kill an agent
    AgentKill = 0x11,
    /// Get own agent info
    AgentSelf = 0x12,

    // ─── Resource operations ───
    /// Read from a capability-gated resource
    ResourceRead = 0x20,
    /// Write to a capability-gated resource
    ResourceWrite = 0x21,

    // ─── System info ───
    /// Get system status (read-only)
    SystemStatus = 0x30,
}

/// Result of a syscall
#[derive(Debug)]
pub enum SyscallResult {
    /// Operation succeeded
    Ok,
    /// Operation succeeded, returning a value
    Value(u64),
    /// Operation succeeded, returning a capability handle
    Handle(CapHandle),
    /// Operation failed
    Err(SyscallError),
}

/// Syscall error codes
#[derive(Debug)]
pub enum SyscallError {
    /// Capability check failed
    CapabilityDenied(CapError),
    /// Invalid syscall number
    InvalidSyscall,
    /// Agent not found
    AgentNotFound,
    /// Budget exceeded
    BudgetExceeded,
    /// Resource unavailable
    ResourceUnavailable,
}

// ─── Syscall dispatcher ─────────────────────────────────────────

/// Execute a capability-gated resource read
pub fn resource_read(
    agent: AgentId,
    cap_handle: CapHandle,
) -> SyscallResult {
    let tick = interrupts::ticks();

    // 1. Check capability
    let table = CAP_TABLE.lock();
    match table.check(cap_handle, Rights::READ, tick) {
        Ok(entry) => {
            let resource_type = entry.resource.describe();
            drop(table);

            // 2. Audit success
            AUDIT_LOG.lock().record(
                tick, agent, AuditEvent::CapGranted, cap_handle, 0,
            );

            // 3. Execute based on resource type
            serial_println!("[syscall] agent:{} READ {} via cap:{}",
                agent.0, resource_type, cap_handle.0);

            SyscallResult::Ok
        }
        Err(e) => {
            drop(table);
            AUDIT_LOG.lock().record(
                tick, agent, AuditEvent::CapCheckFail, cap_handle, -1,
            );
            serial_println!("[guard] DENIED: agent:{} READ cap:{} - {:?}",
                agent.0, cap_handle.0, e);
            SyscallResult::Err(SyscallError::CapabilityDenied(e))
        }
    }
}

/// Execute a capability-gated resource write
pub fn resource_write(
    agent: AgentId,
    cap_handle: CapHandle,
    data: &[u8],
) -> SyscallResult {
    let tick = interrupts::ticks();

    let table = CAP_TABLE.lock();
    match table.check(cap_handle, Rights::WRITE, tick) {
        Ok(entry) => {
            let resource = entry.resource.describe();
            drop(table);

            // Track I/O budget
            let mut agents = crate::agent::AGENT_TABLE.lock();
            if let Some(agent_ref) = agents.get_mut(agent) {
                if !agent_ref.budget.can_io() {
                    AUDIT_LOG.lock().record(
                        tick, agent, AuditEvent::AgentOverBudget, cap_handle, -2,
                    );
                    return SyscallResult::Err(SyscallError::BudgetExceeded);
                }
                agent_ref.budget.record_io();
            }
            drop(agents);

            // Execute write based on resource type
            serial_println!("[syscall] agent:{} WRITE {} ({} bytes) via cap:{}",
                agent.0, resource, data.len(), cap_handle.0);

            AUDIT_LOG.lock().record(
                tick, agent, AuditEvent::CapGranted, cap_handle, 0,
            );

            SyscallResult::Ok
        }
        Err(e) => {
            drop(table);
            AUDIT_LOG.lock().record(
                tick, agent, AuditEvent::CapCheckFail, cap_handle, -1,
            );
            SyscallResult::Err(SyscallError::CapabilityDenied(e))
        }
    }
}

/// Grant a capability to an agent (kernel-only for now)
pub fn cap_grant(
    agent: AgentId,
    resource: ResourceRef,
    rights: Rights,
    timeout_ticks: u64,
) -> SyscallResult {
    let tick = interrupts::ticks();
    let expires = if timeout_ticks > 0 { tick + timeout_ticks } else { 0 };

    let mut table = CAP_TABLE.lock();
    match table.grant(agent, resource, rights, expires, tick) {
        Ok(handle) => {
            drop(table);

            // Add handle to agent's capability set
            let mut agents = crate::agent::AGENT_TABLE.lock();
            if let Some(a) = agents.get_mut(agent) {
                a.add_capability(handle);
            }
            drop(agents);

            AUDIT_LOG.lock().record(
                tick, agent, AuditEvent::CapGranted, handle, 0,
            );

            SyscallResult::Handle(handle)
        }
        Err(e) => {
            AUDIT_LOG.lock().record(
                tick, agent, AuditEvent::CapCheckFail, CapHandle(0), -1,
            );
            SyscallResult::Err(SyscallError::CapabilityDenied(e))
        }
    }
}

/// Revoke a capability
pub fn cap_revoke(agent: AgentId, cap_handle: CapHandle) -> SyscallResult {
    let tick = interrupts::ticks();

    // Verify agent owns this cap
    let table = CAP_TABLE.lock();
    match table.check(cap_handle, Rights::empty(), tick) {
        Ok(entry) => {
            if entry.owner != agent {
                drop(table);
                return SyscallResult::Err(SyscallError::CapabilityDenied(CapError::NotOwner));
            }
            drop(table);
        }
        Err(e) => {
            drop(table);
            return SyscallResult::Err(SyscallError::CapabilityDenied(e));
        }
    }

    let mut table = CAP_TABLE.lock();
    match table.revoke(cap_handle) {
        Ok(()) => {
            drop(table);
            AUDIT_LOG.lock().record(
                tick, agent, AuditEvent::CapRevoked, cap_handle, 0,
            );
            serial_println!("[guard] Revoked cap:{} from agent:{}",
                cap_handle.0, agent.0);
            SyscallResult::Ok
        }
        Err(e) => {
            drop(table);
            SyscallResult::Err(SyscallError::CapabilityDenied(e))
        }
    }
}

/// Get system status
pub fn system_status(agent: AgentId) -> SyscallResult {
    let tick = interrupts::ticks();
    let caps = CAP_TABLE.lock().active_count();
    let agents = crate::agent::AGENT_TABLE.lock().active_count();
    let audit_events = AUDIT_LOG.lock().total_events();

    serial_println!("[agent:{}] System status: ticks={}, caps={}, agents={}, audit={}",
        agent.0, tick, caps, agents, audit_events);

    SyscallResult::Value(tick)
}
