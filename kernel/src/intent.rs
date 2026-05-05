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
use crate::virtfs::FS;
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
        "pipe" | "p" => cmd_pipe(args),
        "send" => cmd_send(args),
        "tick" => cmd_tick(),
        "ls" => cmd_ls(),
        "cat" => cmd_cat(args),
        "write" | "w" => cmd_write(args),
        "rm" => cmd_rm(args),
        "touch" => cmd_touch(args),
        "about" => cmd_about(),
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
    serial_println!("  deploy <name>   Deploy WASM agent");
    serial_println!("  pipe <a> <b>    Chain: a → channel → b");
    serial_println!("  send <msg>      Send to channel:0");
    serial_println!("  ls              List files");
    serial_println!("  cat <file>      Show file contents");
    serial_println!("  write <f> <d>   Write data to file");
    serial_println!("  touch <file>    Create empty file");
    serial_println!("  rm <file>       Delete file");
    serial_println!("  tick            APIC tick counter");
    serial_println!("  about           System info");
    serial_println!("  help (?)        This message");
}

fn cmd_status() {
    let tick = interrupts::ticks();
    let caps = CAP_TABLE.lock().active_count();
    let agents = AGENT_TABLE.lock().active_count();
    let audit = AUDIT_LOG.lock().total_events();
    let fs = FS.lock();
    let files = fs.file_count();
    let fs_size = fs.total_size();
    drop(fs);

    serial_println!("  ┌─────────────────────────────┐");
    serial_println!("  │ TuniCore v0.5.0             │");
    serial_println!("  ├─────────────────────────────┤");
    serial_println!("  │ Tick:    {:<19} │", tick);
    serial_println!("  │ Agents:  {:<19} │", agents);
    serial_println!("  │ Caps:    {:<19} │", caps);
    serial_println!("  │ Audit:   {:<19} │", audit);
    serial_println!("  │ Files:   {:<19} │", files);
    serial_println!("  │ FS used: {:<15} B   │", fs_size);
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
        serial_println!("  Available: hello, sender, receiver, writer");
        return;
    }

    serial_println!("  Deploying '{}'...", name);

    // Use the built-in WASM agents
    static HELLO_WASM: &[u8] = include_bytes!("hello_agent.wasm");
    static SENDER_WASM: &[u8] = include_bytes!("sender_agent.wasm");
    static RECEIVER_WASM: &[u8] = include_bytes!("receiver_agent.wasm");
    static WRITER_WASM: &[u8] = include_bytes!("writer_agent.wasm");

    let (wasm, chan_w, chan_r) = match name {
        "hello" => (HELLO_WASM, None, None),
        "writer" => (WRITER_WASM, None, None),
        "sender" => {
            let chan_id = ensure_channel_0();
            (SENDER_WASM, Some(chan_id), None)
        }
        "receiver" => {
            let chan_id = ensure_channel_0();
            (RECEIVER_WASM, None, Some(chan_id))
        }
        _ => {
            serial_println!("  Unknown agent '{}'. Available: hello, sender, receiver, writer", name);
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

fn cmd_pipe(args: &str) {
    let mut parts = args.splitn(2, ' ');
    let agent_a = parts.next().unwrap_or("").trim();
    let agent_b = parts.next().unwrap_or("").trim();

    if agent_a.is_empty() || agent_b.is_empty() {
        serial_println!("  Usage: pipe <sender> <receiver>");
        serial_println!("  Example: pipe sender receiver");
        return;
    }

    serial_println!("  ─── Pipeline: {} → {} ───", agent_a, agent_b);

    // Create a fresh channel for this pipeline
    let chan_id = {
        let mut channels = CHANNELS.lock();
        match channels.create() {
            Ok(id) => id,
            Err(_) => {
                serial_println!("  Failed to create pipeline channel");
                return;
            }
        }
    };
    serial_println!("  Created channel:{}", chan_id);

    // Resolve WASM bytes
    static HELLO_WASM: &[u8] = include_bytes!("hello_agent.wasm");
    static SENDER_WASM: &[u8] = include_bytes!("sender_agent.wasm");
    static RECEIVER_WASM: &[u8] = include_bytes!("receiver_agent.wasm");
    static WRITER_WASM: &[u8] = include_bytes!("writer_agent.wasm");

    let wasm_a = match agent_a {
        "hello" => Some(HELLO_WASM),
        "sender" => Some(SENDER_WASM),
        "writer" => Some(WRITER_WASM),
        _ => None,
    };

    let wasm_b = match agent_b {
        "hello" => Some(HELLO_WASM),
        "receiver" => Some(RECEIVER_WASM),
        "writer" => Some(WRITER_WASM),
        _ => None,
    };

    if wasm_a.is_none() {
        serial_println!("  Unknown agent: '{}'", agent_a);
        return;
    }
    if wasm_b.is_none() {
        serial_println!("  Unknown agent: '{}'", agent_b);
        return;
    }

    // Run agent A (writes to channel)
    serial_println!("  Running {} → channel:{}...", agent_a, chan_id);
    match crate::wasm_runtime::execute_agent(
        agent_a, wasm_a.unwrap(), None, Some(chan_id), None,
    ) {
        Ok(()) => serial_println!("  {} completed ✓", agent_a),
        Err(e) => {
            serial_println!("  {} failed: {}", agent_a, e);
            return;
        }
    }

    // Check channel
    {
        let channels = CHANNELS.lock();
        if let Some(ch) = channels.get(chan_id) {
            serial_println!("  Channel:{} → {} messages queued", chan_id, ch.message_count());
        }
    }

    // Run agent B (reads from channel)
    serial_println!("  Running {} ← channel:{}...", agent_b, chan_id);
    match crate::wasm_runtime::execute_agent(
        agent_b, wasm_b.unwrap(), None, None, Some(chan_id),
    ) {
        Ok(()) => serial_println!("  {} completed ✓", agent_b),
        Err(e) => serial_println!("  {} failed: {}", agent_b, e),
    }

    serial_println!("  ─── Pipeline complete ───");
}

fn cmd_about() {
    serial_println!("  ╔═══════════════════════════════════╗");
    serial_println!("  ║  TuniCore v0.5.0                  ║");
    serial_println!("  ║  Confidential Agent Runtime       ║");
    serial_println!("  ╠═══════════════════════════════════╣");
    serial_println!("  ║  Architecture: x86_64             ║");
    serial_println!("  ║  APIC: x2APIC (MSR-based)        ║");
    serial_println!("  ║  WASM: wasmi 1.0.9 (pure Rust)   ║");
    serial_println!("  ║  Security: capability-based       ║");
    serial_println!("  ║  Audit: FNV-1a hash chain         ║");
    serial_println!("  ╠═══════════════════════════════════╣");
    serial_println!("  ║  The agent is the interface.      ║");
    serial_println!("  ║  The kernel is the guard.         ║");
    serial_println!("  ╚═══════════════════════════════════╝");
}

/// Ensure channel 0 exists
fn ensure_channel_0() -> u64 {
    let mut channels = CHANNELS.lock();
    if channels.get(0).is_some() {
        return 0;
    }
    channels.create().unwrap_or(0)
}

fn cmd_ls() {
    let fs = FS.lock();
    let files = fs.list();
    if files.is_empty() {
        serial_println!("  (empty filesystem)");
        return;
    }
    serial_println!("  {:20} {:>8}  {:>8}", "NAME", "SIZE", "TICK");
    serial_println!("  {:20} {:>8}  {:>8}", "────", "────", "────");
    for f in files {
        serial_println!("  {:20} {:>6} B  t={}", f.name, f.size(), f.modified_at);
    }
    serial_println!("  {} files, {} bytes total", fs.file_count(), fs.total_size());
}

fn cmd_cat(args: &str) {
    if args.is_empty() {
        serial_println!("  Usage: cat <filename>");
        return;
    }
    let fs = FS.lock();
    match fs.read(args) {
        Some(data) => {
            if let Ok(text) = core::str::from_utf8(data) {
                serial_println!("{}", text);
            } else {
                serial_println!("  ({} bytes, binary)", data.len());
            }
        }
        None => serial_println!("  File '{}' not found", args),
    }
}

fn cmd_write(args: &str) {
    // Format: write <filename> <content>
    let mut parts = args.splitn(2, ' ');
    let name = parts.next().unwrap_or("");
    let content = parts.next().unwrap_or("");

    if name.is_empty() {
        serial_println!("  Usage: write <filename> <content>");
        return;
    }

    let tick = interrupts::ticks();
    let mut fs = FS.lock();
    match fs.write(name, content.as_bytes(), tick) {
        Ok(()) => serial_println!("  Wrote {} bytes to '{}'", content.len(), name),
        Err(e) => serial_println!("  Write failed: {}", e),
    }
}

fn cmd_touch(args: &str) {
    if args.is_empty() {
        serial_println!("  Usage: touch <filename>");
        return;
    }
    let tick = interrupts::ticks();
    let mut fs = FS.lock();
    match fs.touch(args, tick) {
        Ok(()) => serial_println!("  Created '{}'", args),
        Err(e) => serial_println!("  Touch failed: {}", e),
    }
}

fn cmd_rm(args: &str) {
    if args.is_empty() {
        serial_println!("  Usage: rm <filename>");
        return;
    }
    let mut fs = FS.lock();
    match fs.remove(args) {
        Ok(()) => serial_println!("  Removed '{}'", args),
        Err(e) => serial_println!("  Remove failed: {}", e),
    }
}
