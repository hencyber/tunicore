//! WASM Agent Runtime — Phase 5
//!
//! Sandboxed WASM execution with rich host functions:
//! - tc.log(ptr, len)          — write to serial
//! - tc.time() -> i64          — get kernel tick
//! - tc.chan_send(id, ptr, len) -> i32  — send to channel
//! - tc.chan_recv(id, ptr, len) -> i32  — receive from channel

use wasmi::{Caller, Engine, Extern, Func, Linker, Module, Store, TypedFunc};

use crate::agent::{AgentState, ResourceBudget, AGENT_TABLE};
use crate::audit::{AuditEvent, AUDIT_LOG};
use crate::cap_table::{AgentId, CapHandle, CAP_TABLE};
use crate::capability::types::Rights;
use crate::channel::{self, Message, CHANNELS};
use crate::interrupts;
use crate::resource::ResourceRef;
use crate::serial_println;

/// Host state passed to WASM host functions
struct HostState {
    agent_id: AgentId,
    serial_cap: Option<CapHandle>,
    channel_write_cap: Option<(CapHandle, u64)>, // (cap, channel_id)
    channel_read_cap: Option<(CapHandle, u64)>,
}

/// Execute a WASM agent with optional channel capabilities
pub fn execute_agent(
    name: &str,
    wasm_bytes: &[u8],
    parent: Option<AgentId>,
    chan_write: Option<u64>,
    chan_read: Option<u64>,
) -> Result<(), &'static str> {
    let tick = interrupts::ticks();
    serial_println!("[wasm] Loading agent: \"{}\" ({} bytes)", name, wasm_bytes.len());

    // 1. Spawn agent
    let agent_id = {
        let mut table = AGENT_TABLE.lock();
        table.spawn(name, parent, ResourceBudget::default_budget(), 60_000, tick)?
    };

    AUDIT_LOG.lock().record(tick, agent_id, AuditEvent::AgentSpawned, CapHandle(0), 0);

    // 2. Grant Serial(W) cap
    let serial_cap = {
        let mut ct = CAP_TABLE.lock();
        ct.grant(agent_id, ResourceRef::Serial, Rights::WRITE, 0, tick).ok()
    };

    // 3. Grant channel caps if requested
    let channel_write_cap = if let Some(chan_id) = chan_write {
        let mut ct = CAP_TABLE.lock();
        ct.grant(agent_id, ResourceRef::Channel { channel_id: chan_id }, Rights::WRITE, 0, tick)
            .ok()
            .map(|cap| {
                serial_println!("[wasm] Granted cap:{} Channel:{}(W) to agent:{}", cap.0, chan_id, agent_id.0);
                (cap, chan_id)
            })
    } else {
        None
    };

    let channel_read_cap = if let Some(chan_id) = chan_read {
        let mut ct = CAP_TABLE.lock();
        ct.grant(agent_id, ResourceRef::Channel { channel_id: chan_id }, Rights::READ, 0, tick)
            .ok()
            .map(|cap| {
                serial_println!("[wasm] Granted cap:{} Channel:{}(R) to agent:{}", cap.0, chan_id, agent_id.0);
                (cap, chan_id)
            })
    } else {
        None
    };

    // 4. Set up WASM engine
    let engine = Engine::default();
    let module = Module::new(&engine, wasm_bytes).map_err(|_| "WASM parse error")?;

    let host = HostState { agent_id, serial_cap, channel_write_cap, channel_read_cap };
    let mut store = Store::new(&engine, host);
    let mut linker = <Linker<HostState>>::new(&engine);

    // ─── Host function: tc.log ───
    linker.define("tc", "log",
        Func::wrap(&mut store, |caller: Caller<'_, HostState>, ptr: i32, len: i32| {
            let aid = caller.data().agent_id;
            let cap = caller.data().serial_cap;
            if let Some(cap) = cap {
                let tick = interrupts::ticks();
                let ok = CAP_TABLE.lock().check(cap, Rights::WRITE, tick).is_ok();
                if ok {
                    if let Some(mem) = caller.get_export("memory").and_then(Extern::into_memory) {
                        let mut buf = [0u8; 256];
                        let n = (len as usize).min(255);
                        if mem.read(&caller, ptr as usize, &mut buf[..n]).is_ok() {
                            if let Ok(s) = core::str::from_utf8(&buf[..n]) {
                                serial_println!("[agent:{}] {}", aid.0, s);
                            }
                        }
                    }
                }
            }
        }),
    ).map_err(|_| "link tc.log")?;

    // ─── Host function: tc.time ───
    linker.define("tc", "time",
        Func::wrap(&mut store, |_caller: Caller<'_, HostState>| -> i64 {
            interrupts::ticks() as i64
        }),
    ).map_err(|_| "link tc.time")?;

    // ─── Host function: tc.chan_send ───
    linker.define("tc", "chan_send",
        Func::wrap(&mut store, |caller: Caller<'_, HostState>, _chan: i32, ptr: i32, len: i32| -> i32 {
            let aid = caller.data().agent_id;
            let cap_info = caller.data().channel_write_cap;

            if let Some((cap, chan_id)) = cap_info {
                let tick = interrupts::ticks();
                let ok = CAP_TABLE.lock().check(cap, Rights::WRITE, tick).is_ok();
                if ok {
                    // Read data from WASM memory
                    let mut buf = [0u8; 256];
                    let n = (len as usize).min(256);
                    if let Some(mem) = caller.get_export("memory").and_then(Extern::into_memory) {
                        if mem.read(&caller, ptr as usize, &mut buf[..n]).is_ok() {
                            let msg = Message::new(aid, &buf[..n], tick);
                            let mut channels = CHANNELS.lock();
                            if let Some(ch) = channels.get_mut(chan_id) {
                                if ch.send(msg).is_ok() {
                                    return n as i32;
                                }
                            }
                        }
                    }
                }
            }
            -1 // Error
        }),
    ).map_err(|_| "link tc.chan_send")?;

    // ─── Host function: tc.chan_recv ───
    linker.define("tc", "chan_recv",
        Func::wrap(&mut store, |mut caller: Caller<'_, HostState>, _chan: i32, ptr: i32, len: i32| -> i32 {
            let aid = caller.data().agent_id;
            let cap_info = caller.data().channel_read_cap;

            if let Some((cap, chan_id)) = cap_info {
                let tick = interrupts::ticks();
                let ok = CAP_TABLE.lock().check(cap, Rights::READ, tick).is_ok();
                if ok {
                    let mut channels = CHANNELS.lock();
                    if let Some(ch) = channels.get_mut(chan_id) {
                        if let Some(msg) = ch.recv() {
                            let n = msg.len.min(len as usize);
                            drop(channels);
                            if let Some(mem) = caller.get_export("memory").and_then(Extern::into_memory) {
                                if mem.write(&mut caller, ptr as usize, &msg.data[..n]).is_ok() {
                                    return n as i32;
                                }
                            }
                        }
                    }
                }
            }
            -1 // No message
        }),
    ).map_err(|_| "link tc.chan_recv")?;

    // 5. Instantiate and run
    let instance = linker.instantiate_and_start(&mut store, &module)
        .map_err(|_| "WASM instantiation error")?;

    {
        let mut table = AGENT_TABLE.lock();
        if let Some(a) = table.get_mut(agent_id) { a.set_state(AgentState::Active); }
    }

    serial_println!("[wasm] Executing agent:{}...", agent_id.0);

    let start_fn: TypedFunc<(), ()> = instance
        .get_typed_func::<(), ()>(&store, "_start")
        .map_err(|_| "no _start")?;

    match start_fn.call(&mut store, ()) {
        Ok(()) => serial_println!("[wasm] Agent \"{}\" completed.", name),
        Err(e) => serial_println!("[wasm] Agent \"{}\" trapped: {}", name, e),
    }

    // Cleanup
    { let mut t = AGENT_TABLE.lock(); let _ = t.kill(agent_id); }
    AUDIT_LOG.lock().record(interrupts::ticks(), agent_id, AuditEvent::AgentKilled, CapHandle(0), 0);

    Ok(())
}
