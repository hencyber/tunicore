//! WASM Agent Runtime
//!
//! Executes sandboxed WASM agents with capability-gated host functions.
//! This is where TuniCore becomes a real agent runtime:
//! - Agent code is loaded at runtime (not compiled into kernel)
//! - Memory is sandboxed (WASM linear memory, can't escape)
//! - Resource access only through host functions (capability-checked)
//!
//! Uses wasmi (pure Rust, no_std) as the WASM interpreter.

use alloc::string::String;
use wasmi::{
    Caller, Engine, Extern, Func, Linker, Memory, Module, Store, TypedFunc,
};

use crate::agent::{AgentState, ResourceBudget, AGENT_TABLE};
use crate::audit::{AuditEvent, AUDIT_LOG};
use crate::cap_table::{AgentId, CapHandle, CAP_TABLE};
use crate::capability::types::Rights;
use crate::interrupts;
use crate::resource::ResourceRef;
use crate::serial_println;

/// Host state passed to WASM host functions
struct HostState {
    /// Agent ID of the running WASM agent
    agent_id: AgentId,
    /// Capability handle for serial output
    serial_cap: Option<CapHandle>,
}

/// Execute a WASM agent module
///
/// # Arguments
/// * `name` - Human-readable agent name
/// * `wasm_bytes` - Raw WASM bytecode
/// * `parent` - Parent agent ID (or None for root-spawned)
///
/// # Returns
/// Ok(()) on success, Err with message on failure
pub fn execute_agent(
    name: &str,
    wasm_bytes: &[u8],
    parent: Option<AgentId>,
) -> Result<(), &'static str> {
    let tick = interrupts::ticks();

    serial_println!("[wasm] Loading agent: \"{}\" ({} bytes)", name, wasm_bytes.len());

    // 1. Spawn agent in the agent table
    let agent_id = {
        let mut table = AGENT_TABLE.lock();
        table.spawn(
            name,
            parent,
            ResourceBudget::default_budget(),
            60_000, // 60 second lifetime
            tick,
        )?
    };

    // Record spawn event
    AUDIT_LOG.lock().record(
        tick, agent_id, AuditEvent::AgentSpawned, CapHandle(0), 0,
    );

    // 2. Grant capabilities
    let serial_cap = {
        let mut cap_table = CAP_TABLE.lock();
        cap_table.grant(
            agent_id,
            ResourceRef::Serial,
            Rights::WRITE,
            0, // No expiry for now
            tick,
        ).ok()
    };

    if let Some(cap) = serial_cap {
        serial_println!("[wasm] Granted cap:{} Serial(W) to agent:{}", cap.0, agent_id.0);
        AUDIT_LOG.lock().record(
            tick, agent_id, AuditEvent::CapGranted, cap, 0,
        );
    }

    // 3. Set up WASM engine
    let engine = Engine::default();

    let module = Module::new(&engine, wasm_bytes)
        .map_err(|_| "WASM module parse error")?;

    let host_state = HostState {
        agent_id,
        serial_cap,
    };

    let mut store = Store::new(&engine, host_state);

    // 4. Define host functions via Linker
    let mut linker = <Linker<HostState>>::new(&engine);

    // tc.log(ptr: i32, len: i32) — write string to serial via capability
    linker.define(
        "tc", "log",
        Func::wrap(&mut store, |caller: Caller<'_, HostState>, ptr: i32, len: i32| {
            let host = caller.data();
            let agent_id = host.agent_id;
            let serial_cap = host.serial_cap;

            // Check capability
            if let Some(cap) = serial_cap {
                let tick = interrupts::ticks();
                let table = CAP_TABLE.lock();
                match table.check(cap, Rights::WRITE, tick) {
                    Ok(_) => {
                        drop(table);
                        // Read string from WASM memory
                        if let Some(memory) = caller.get_export("memory").and_then(Extern::into_memory) {
                            let mut buf = [0u8; 256];
                            let read_len = (len as usize).min(255);
                            if memory.read(&caller, ptr as usize, &mut buf[..read_len]).is_ok() {
                                if let Ok(msg) = core::str::from_utf8(&buf[..read_len]) {
                                    serial_println!("[agent:{}] {}", agent_id.0, msg);
                                }
                            }
                        }

                        // Record in audit
                        AUDIT_LOG.lock().record(
                            tick, agent_id, AuditEvent::CapGranted, cap, 0,
                        );
                    }
                    Err(_) => {
                        drop(table);
                        serial_println!("[guard] DENIED: agent:{} log write", agent_id.0);
                    }
                }
            }
        }),
    ).map_err(|_| "failed to define tc.log")?;

    // 5. Instantiate
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| "WASM instantiation error")?;

    // 6. Mark agent as active
    {
        let mut table = AGENT_TABLE.lock();
        if let Some(agent) = table.get_mut(agent_id) {
            agent.set_state(AgentState::Active);
        }
    }

    serial_println!("[wasm] Executing agent:{}...", agent_id.0);

    // 7. Call _start
    let start_fn: TypedFunc<(), ()> = instance
        .get_typed_func::<(), ()>(&store, "_start")
        .map_err(|_| "no _start export")?;

    let exec_result = start_fn.call(&mut store, ());

    // 8. Finalize
    let final_tick = interrupts::ticks();
    let audit_count = AUDIT_LOG.lock().total_events();

    match exec_result {
        Ok(()) => {
            serial_println!("[wasm] Agent \"{}\" completed successfully.", name);
        }
        Err(e) => {
            serial_println!("[wasm] Agent \"{}\" trapped: {}", name, e);
        }
    }

    // 9. Kill agent + revoke all caps
    {
        let mut table = AGENT_TABLE.lock();
        let _ = table.kill(agent_id);
    }

    AUDIT_LOG.lock().record(
        final_tick, agent_id, AuditEvent::AgentKilled, CapHandle(0), 0,
    );

    serial_println!("[wasm] Agent cleaned up. Audit events: {}", audit_count);

    Ok(())
}
