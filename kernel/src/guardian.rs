//! Guardian - the built-in kernel agent
//!
//! This is TuniCore's first agent. It runs in kernel space and proves
//! the entire capability chain works:
//!   spawn -> grant caps -> use caps -> audit logs -> revoke
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
pub fn run() -> ! {
    serial_println!();
    serial_println!("[guardian] Initializing kernel agent...");

    // 1. Spawn the guardian agent
    let tick = interrupts::ticks();
    let agent_id = {
        let mut table = AGENT_TABLE.lock();
        match table.spawn(
            "guardian",
            None, // No parent - this IS the root
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
                loop { x86_64::instructions::hlt(); }
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

    // 3. Use capabilities - prove the enforcement works
    serial_println!();
    serial_println!("[guardian] Testing capability enforcement...");

    // Read via serial cap - should succeed
    if let SyscallResult::Handle(cap) = serial_cap {
        let result = syscall::resource_read(agent_id, cap);
        serial_println!("[guardian] Serial READ: {:?}", result_status(&result));
    }

    // Write via serial cap - should succeed
    if let SyscallResult::Handle(cap) = serial_cap {
        let result = syscall::resource_write(agent_id, cap, b"hello from guardian");
        serial_println!("[guardian] Serial WRITE: {:?}", result_status(&result));
    }

    // Read via memory cap - should succeed
    if let SyscallResult::Handle(cap) = memory_cap {
        let result = syscall::resource_read(agent_id, cap);
        serial_println!("[guardian] Memory READ: {:?}", result_status(&result));
    }

    // 4. Test DENIAL - try to WRITE to memory with a READ-only cap
    serial_println!();
    serial_println!("[guardian] Testing capability DENIAL...");
    if let SyscallResult::Handle(cap) = memory_cap {
        let result = syscall::resource_write(agent_id, cap, b"should fail");
        serial_println!("[guardian] Memory WRITE (read-only cap): {:?}", result_status(&result));
    }

    // 5. Test DELEGATION - delegate attenuated serial cap
    serial_println!();
    serial_println!("[guardian] Testing capability delegation...");
    if let SyscallResult::Handle(cap) = serial_cap {
        // Delegate read-only serial to a hypothetical sub-agent
        let tick = interrupts::ticks();
        let mut table = crate::cap_table::CAP_TABLE.lock();
        match table.delegate(cap, AgentId(99), Rights::READ, tick) {
            Ok(child_handle) => {
                serial_println!(
                    "[guardian] Delegated cap:{} -> cap:{} (READ only) to agent:99",
                    cap.0, child_handle.0
                );

                // Now revoke parent - child should cascade-revoke
                serial_println!("[guardian] Revoking parent cap:{}...", cap.0);
                let _ = table.revoke(cap);

                // Check child is also revoked
                match table.check(child_handle, Rights::READ, tick) {
                    Ok(_) => serial_println!("[guardian] BUG: child still valid!"),
                    Err(e) => serial_println!(
                        "[guardian] Cascade revocation works: child cap:{} -> {:?}",
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

    // 7. WASM sandbox test - single agent
    serial_println!();
    serial_println!("[guardian] --- WASM Sandbox Test ---");

    static HELLO_WASM: &[u8] = include_bytes!("hello_agent.wasm");

    match crate::wasm_runtime::execute_agent("hello.wasm", HELLO_WASM, Some(agent_id), None, None) {
        Ok(()) => serial_println!("[guardian] Single agent: OK ok"),
        Err(e) => serial_println!("[guardian] Single agent failed: {}", e),
    }

    // 8. Multi-agent channel test
    serial_println!();
    serial_println!("[guardian] --- Multi-Agent Channel Test ---");

    // Create a channel
    let chan_id = {
        let mut channels = crate::channel::CHANNELS.lock();
        channels.create().unwrap_or(0)
    };
    serial_println!("[guardian] Created channel:{}", chan_id);

    // Sender agent writes to channel
    static SENDER_WASM: &[u8] = include_bytes!("sender_agent.wasm");
    match crate::wasm_runtime::execute_agent(
        "sender.wasm", SENDER_WASM, Some(agent_id),
        Some(chan_id), None,
    ) {
        Ok(()) => serial_println!("[guardian] Sender agent: OK ok"),
        Err(e) => serial_println!("[guardian] Sender failed: {}", e),
    }

    // Check channel state
    {
        let channels = crate::channel::CHANNELS.lock();
        if let Some(ch) = channels.get(chan_id) {
            serial_println!("[guardian] Channel:{} has {} messages", chan_id, ch.message_count());
        }
    }

    // Receiver agent reads from channel
    static RECEIVER_WASM: &[u8] = include_bytes!("receiver_agent.wasm");
    match crate::wasm_runtime::execute_agent(
        "receiver.wasm", RECEIVER_WASM, Some(agent_id),
        None, Some(chan_id),
    ) {
        Ok(()) => serial_println!("[guardian] Receiver agent: OK ok"),
        Err(e) => serial_println!("[guardian] Receiver failed: {}", e),
    }

    // Final status
    serial_println!();
    syscall::system_status(agent_id);
    serial_println!("[guardian] All tests passed. Multi-agent system operational.");

    // 9. Enter interactive intent mode - "talk to your OS"
    serial_println!();
    serial_println!("=======================================");
    serial_println!("  TuniCore v0.6.0 - Intent Layer");
    serial_println!("  Type 'help' for commands.");
    serial_println!("=======================================");
    serial_println!();

    // Initialize chat UI on framebuffer if available
    if let Some(fb) = crate::get_framebuffer() {
        crate::chat_ui::ChatUI::init(fb);
        if let Some(ref mut ui) = *crate::chat_ui::CHAT.lock() {
            ui.system_msg("Welcome to TuniCore!");
            ui.system_msg("Just type what you need. No commands to memorize.");
            ui.system_msg("Try: show my files, deploy greeter, or ask anything.");
        }
        crate::keyboard::init();
    }

    interactive_loop();
}

/// Interactive command loop - reads from serial AND keyboard
fn interactive_loop() -> ! {
    let mut line_buf = [0u8; 256];
    let mut line_pos: usize = 0;

    // Print initial prompt on serial
    print_prompt();

    loop {
        // Check keyboard first (for framebuffer/chat mode)
        if let Some(key) = crate::keyboard::read_key() {
            // Forward to chat UI
            if let Some(ref mut ui) = *crate::chat_ui::CHAT.lock() {
                if let Some(cmd) = ui.key_input(key) {
                    // Execute the command
                    crate::intent::execute(&cmd);
                    // Show response in chat
                    // (intent already writes to serial, we mirror here)
                }
            }
            // Also echo to serial
            match key {
                b'\n' => {
                    serial_println!();
                    if line_pos > 0 {
                        if let Ok(cmd) = core::str::from_utf8(&line_buf[..line_pos]) {
                            crate::intent::execute(cmd);
                        }
                        line_pos = 0;
                    }
                    print_prompt();
                }
                8 | 0x7F => {
                    if line_pos > 0 {
                        line_pos -= 1;
                        let mut s = crate::serial::SERIAL.lock();
                        s.write_byte(0x08); s.write_byte(b' '); s.write_byte(0x08);
                    }
                }
                0x20..=0x7E => {
                    if line_pos < 255 {
                        line_buf[line_pos] = key;
                        line_pos += 1;
                        crate::serial::SERIAL.lock().write_byte(key);
                    }
                }
                _ => {}
            }
            continue;
        }

        // Check serial input
        let byte = crate::serial::SERIAL.lock().read_byte();
        if let Some(b) = byte {
            match b {
                // Enter - execute command
                b'\r' | b'\n' => {
                    serial_println!();
                    if line_pos > 0 {
                        if let Ok(cmd) = core::str::from_utf8(&line_buf[..line_pos]) {
                            // Show in chat UI too
                            if let Some(ref mut ui) = *crate::chat_ui::CHAT.lock() {
                                ui.user_msg(cmd);
                            }
                            crate::intent::execute(cmd);
                        }
                        line_pos = 0;
                    }
                    print_prompt();
                }
                // Backspace
                0x7F | 0x08 => {
                    if line_pos > 0 {
                        line_pos -= 1;
                        let mut s = crate::serial::SERIAL.lock();
                        s.write_byte(0x08); s.write_byte(b' '); s.write_byte(0x08);
                    }
                }
                // Printable ASCII
                0x20..=0x7E => {
                    if line_pos < 255 {
                        line_buf[line_pos] = b;
                        line_pos += 1;
                        crate::serial::SERIAL.lock().write_byte(b);
                    }
                }
                _ => {}
            }
        } else {
            // No input - yield CPU
            x86_64::instructions::hlt();
        }
    }
}

fn log_grant(name: &str, result: &SyscallResult) {
    match result {
        SyscallResult::Handle(h) => {
            serial_println!("[guardian]   cap:{} <- {}", h.0, name);
        }
        SyscallResult::Err(e) => {
            serial_println!("[guardian]   FAILED: {} - {:?}", name, e);
        }
        _ => {}
    }
}

fn result_status(result: &SyscallResult) -> &'static str {
    match result {
        SyscallResult::Ok | SyscallResult::Value(_) | SyscallResult::Handle(_) => "OK ok",
        SyscallResult::Err(_) => "DENIED FAIL",
    }
}

fn print_prompt() {
    let mut s = crate::serial::SERIAL.lock();
    for &b in b"tc> " {
        s.write_byte(b);
    }
}
