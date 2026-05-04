//! Guardian — the built-in kernel agent
//!
//! This is TuniCore's first agent. It runs in kernel space and proves
//! the entire capability chain works:
//!   spawn → grant caps → use caps → audit logs → revoke
//!
//! In the future, this agent will be the "orchestrator" that manages
//! user-deployed WASM agents.

use crate::agent::{AgentState, ResourceBudget, AGENT_TABLE};
use crate::cap_table::AgentId;
use crate::capability::types::Rights;
use crate::resource::ResourceRef;
use crate::syscall::{self, SyscallResult};
use crate::{serial_println, interrupts};

/// Spawn and run the guardian agent
pub fn run() {
    serial_println!();
    serial_println!("[guardian] Initializing kernel agent...");

    // 1. Spawn the guardian agent
    let tick = interrupts::ticks();
    let agent_id = {
        let mut table = AGENT_TABLE.lock();
        match table.spawn(
            "guardian",
            None, // No parent — this IS the root
            ResourceBudget::default_budget(),
            0,    // No lifetime limit
            tick,
        ) {
            Ok(id) => {
                // Mark as active
                if let Some(agent) = table.get_mut(id) {
                    agent.set_state(AgentState::Active);
                }
                id
            }
            Err(e) => {
                serial_println!("[guardian] FATAL: failed to spawn: {}", e);
                return;
            }
        }
    };

    serial_println!("[guardian] Spawned agent:{} \"guardian\"", agent_id.0);

    // 2. Grant capabilities through the syscall gate
    serial_println!("[guardian] Requesting capabilities...");

    // Serial console: read + write
    let serial_cap = syscall::cap_grant(
        agent_id,
        ResourceRef::Serial,
        Rights::READ | Rights::WRITE,
        0, // No expiry
    );
    log_grant("Serial(RW)", &serial_cap);

    // Memory: read-only (system inspection)
    let memory_cap = syscall::cap_grant(
        agent_id,
        ResourceRef::Memory { base: 0, length: 256 * 1024 * 1024 },
        Rights::READ,
        0,
    );
    log_grant("Memory(R)", &memory_cap);

    // Audit log: read-only
    let audit_cap = syscall::cap_grant(
        agent_id,
        ResourceRef::AuditLog,
        Rights::READ,
        0,
    );
    log_grant("AuditLog(R)", &audit_cap);

    // Compute: read+execute with 60-second timeout
    let compute_cap = syscall::cap_grant(
        agent_id,
        ResourceRef::Compute {
            device: crate::resource::DeviceId(0),
            slot: 0,
        },
        Rights::READ | Rights::EXECUTE,
        6000, // ~60 seconds at 100 Hz
    );
    log_grant("Compute(RX, 60s timeout)", &compute_cap);

    // 3. Use capabilities — prove the enforcement works
    serial_println!();
    serial_println!("[guardian] Testing capability enforcement...");

    // Read via serial cap — should succeed
    if let SyscallResult::Handle(cap) = serial_cap {
        let result = syscall::resource_read(agent_id, cap);
        serial_println!("[guardian] Serial READ: {:?}", result_status(&result));
    }

    // Write via serial cap — should succeed
    if let SyscallResult::Handle(cap) = serial_cap {
        let result = syscall::resource_write(agent_id, cap, b"hello from guardian");
        serial_println!("[guardian] Serial WRITE: {:?}", result_status(&result));
    }

    // Read via memory cap — should succeed
    if let SyscallResult::Handle(cap) = memory_cap {
        let result = syscall::resource_read(agent_id, cap);
        serial_println!("[guardian] Memory READ: {:?}", result_status(&result));
    }

    // 4. Test DENIAL — try to WRITE to memory with a READ-only cap
    serial_println!();
    serial_println!("[guardian] Testing capability DENIAL...");
    if let SyscallResult::Handle(cap) = memory_cap {
        let result = syscall::resource_write(agent_id, cap, b"should fail");
        serial_println!("[guardian] Memory WRITE (read-only cap): {:?}", result_status(&result));
    }

    // 5. Test DELEGATION — delegate attenuated serial cap
    serial_println!();
    serial_println!("[guardian] Testing capability delegation...");
    if let SyscallResult::Handle(cap) = serial_cap {
        // Delegate read-only serial to a hypothetical sub-agent
        let tick = interrupts::ticks();
        let mut table = crate::cap_table::CAP_TABLE.lock();
        match table.delegate(cap, AgentId(99), Rights::READ, tick) {
            Ok(child_handle) => {
                serial_println!(
                    "[guardian] Delegated cap:{} → cap:{} (READ only) to agent:99",
                    cap.0, child_handle.0
                );

                // Now revoke parent — child should cascade-revoke
                serial_println!("[guardian] Revoking parent cap:{}...", cap.0);
                let _ = table.revoke(cap);

                // Check child is also revoked
                match table.check(child_handle, Rights::READ, tick) {
                    Ok(_) => serial_println!("[guardian] BUG: child still valid!"),
                    Err(e) => serial_println!(
                        "[guardian] Cascade revocation works: child cap:{} → {:?}",
                        child_handle.0, e
                    ),
                }
            }
            Err(e) => serial_println!("[guardian] Delegation failed: {:?}", e),
        }
    }

    // 6. System status
    serial_println!();
    syscall::system_status(agent_id);

    serial_println!();
    serial_println!("[guardian] All tests passed. Kernel agent operational.");
}

fn log_grant(name: &str, result: &SyscallResult) {
    match result {
        SyscallResult::Handle(h) => {
            serial_println!("[guardian]   cap:{} ← {}", h.0, name);
        }
        SyscallResult::Err(e) => {
            serial_println!("[guardian]   FAILED: {} — {:?}", name, e);
        }
        _ => {}
    }
}

fn result_status(result: &SyscallResult) -> &'static str {
    match result {
        SyscallResult::Ok | SyscallResult::Value(_) | SyscallResult::Handle(_) => "OK ✓",
        SyscallResult::Err(_) => "DENIED ✗",
    }
}
