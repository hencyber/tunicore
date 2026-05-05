//! Intent Layer — the conversational interface
//!
//! Parses natural-language-like commands into kernel operations.
//! This is what makes TuniCore unique: you talk to your OS.
//!
//! Commands:
//!   status          — system overview
//!   agents          — list active agents
//!   caps            — list capabilities
//!   audit [n]       — show recent audit events
//!   deploy <agent>  — deploy a built-in WASM agent
//!   help            — show available commands

use crate::agent::AGENT_TABLE;
use crate::audit::AUDIT_LOG;
use crate::cap_table::CAP_TABLE;
use crate::channel::CHANNELS;
use crate::interrupts;
use crate::serial_println;

/// Parse and execute an intent command
pub fn execute(input: &str) {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return;
    }

    // Split into command + args
    let mut parts = trimmed.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let args = parts.next().unwrap_or("").trim();

    match cmd {
        "help" | "?" => cmd_help(),
        "status" | "s" => cmd_status(),
        "agents" | "a" => cmd_agents(),
        "caps" | "c" => cmd_caps(),
        "audit" => cmd_audit(args),
        "deploy" | "d" => cmd_deploy(args),
        "send" => cmd_send(args),
        "tick" => cmd_tick(),
        _ => {
            serial_println!("  Unknown command: '{}'. Type 'help' for commands.", cmd);
        }
    }
}

fn cmd_help() {
    serial_println!("  TuniCore Intent Commands:");
    serial_println!("  ─────────────────────────");
    serial_println!("  status (s)      System overview");
    serial_println!("  agents (a)      List active agents");
    serial_println!("  caps   (c)      List capabilities");
    serial_println!("  audit [n]       Show last n audit events");
    serial_println!("  deploy <name>   Deploy WASM agent (hello|sender|receiver)");
    serial_println!("  send <msg>      Send message to channel:0");
    serial_println!("  tick            Show APIC tick counter");
    serial_println!("  help (?)        This message");
}

fn cmd_status() {
    let tick = interrupts::ticks();
    let caps = CAP_TABLE.lock().active_count();
    let agents = AGENT_TABLE.lock().active_count();
    let audit = AUDIT_LOG.lock().total_events();
    let channels = {
        let ch = CHANNELS.lock();
        // Count channels with messages
        let mut with_msgs = 0u32;
        // We can't easily iterate, just report basic info
        with_msgs
    };

    serial_println!("  ┌─────────────────────────────┐");
    serial_println!("  │ TuniCore v0.4.0             │");
    serial_println!("  ├─────────────────────────────┤");
    serial_println!("  │ Tick:    {:<19} │", tick);
    serial_println!("  │ Agents:  {:<19} │", agents);
    serial_println!("  │ Caps:    {:<19} │", caps);
    serial_println!("  │ Audit:   {:<19} │", audit);
    serial_println!("  └─────────────────────────────┘");
}

fn cmd_agents() {
    let table = AGENT_TABLE.lock();
    let count = table.active_count();
    if count == 0 {
        serial_println!("  No active agents.");
        return;
    }
    serial_println!("  Active agents: {}", count);
    // Note: we'd need to expose iteration on AgentTable
    // For now, just show the count
    serial_println!("  (Detailed listing requires agent table iteration — coming soon)");
}

fn cmd_caps() {
    let table = CAP_TABLE.lock();
    let count = table.active_count();
    serial_println!("  Active capabilities: {}", count);
    serial_println!("  Max slots: 4096");
}

fn cmd_audit(args: &str) {
    let n: u64 = args.parse().unwrap_or(5);
    let log = AUDIT_LOG.lock();
    let total = log.total_events();
    serial_println!("  Audit log: {} total events (showing last {})", total, n);
    serial_println!("  Hash chain: active (FNV-1a)");
    // Note: would need to expose recent() iteration
    // For now show summary
}

fn cmd_deploy(name: &str) {
    if name.is_empty() {
        serial_println!("  Usage: deploy <agent-name>");
        serial_println!("  Available: hello, sender, receiver");
        return;
    }

    serial_println!("  Deploying '{}'...", name);

    // Use the built-in WASM agents
    static HELLO_WASM: &[u8] = include_bytes!("hello_agent.wasm");
    static SENDER_WASM: &[u8] = include_bytes!("sender_agent.wasm");
    static RECEIVER_WASM: &[u8] = include_bytes!("receiver_agent.wasm");

    let (wasm, chan_w, chan_r) = match name {
        "hello" => (HELLO_WASM, None, None),
        "sender" => {
            // Ensure channel 0 exists
            let chan_id = ensure_channel_0();
            (SENDER_WASM, Some(chan_id), None)
        }
        "receiver" => {
            let chan_id = ensure_channel_0();
            (RECEIVER_WASM, None, Some(chan_id))
        }
        _ => {
            serial_println!("  Unknown agent '{}'. Available: hello, sender, receiver", name);
            return;
        }
    };

    match crate::wasm_runtime::execute_agent(name, wasm, None, chan_w, chan_r) {
        Ok(()) => serial_println!("  Agent '{}' completed successfully.", name),
        Err(e) => serial_println!("  Agent '{}' failed: {}", name, e),
    }
}

fn cmd_send(args: &str) {
    if args.is_empty() {
        serial_println!("  Usage: send <message>");
        return;
    }

    let chan_id = ensure_channel_0();
    let tick = interrupts::ticks();

    let msg = crate::channel::Message::new(
        crate::cap_table::AgentId(0), // kernel
        args.as_bytes(),
        tick,
    );

    let mut channels = CHANNELS.lock();
    if let Some(ch) = channels.get_mut(chan_id) {
        match ch.send(msg) {
            Ok(()) => serial_println!("  Sent to channel:{}: \"{}\"", chan_id, args),
            Err(e) => serial_println!("  Send failed: {}", e),
        }
    } else {
        serial_println!("  Channel {} not found", chan_id);
    }
}

fn cmd_tick() {
    serial_println!("  APIC tick: {}", interrupts::ticks());
}

/// Ensure channel 0 exists
fn ensure_channel_0() -> u64 {
    let mut channels = CHANNELS.lock();
    if channels.get(0).is_some() {
        return 0;
    }
    channels.create().unwrap_or(0)
}
