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
use crate::alias::ALIASES;
use crate::env::ENV;
use crate::virtfs::FS;
use crate::interrupts;
use crate::serial_println;

/// Command history ring buffer
const HISTORY_SIZE: usize = 32;
static HISTORY: spin::Mutex<History> = spin::Mutex::new(History::new());

struct History {
    entries: [[u8; 128]; HISTORY_SIZE],
    lens: [usize; HISTORY_SIZE],
    head: usize,
    count: usize,
}

impl History {
    const fn new() -> Self {
        Self {
            entries: [[0u8; 128]; HISTORY_SIZE],
            lens: [0; HISTORY_SIZE],
            head: 0,
            count: 0,
        }
    }

    fn push(&mut self, cmd: &str) {
        let n = cmd.len().min(127);
        self.entries[self.head][..n].copy_from_slice(&cmd.as_bytes()[..n]);
        self.lens[self.head] = n;
        self.head = (self.head + 1) % HISTORY_SIZE;
        if self.count < HISTORY_SIZE { self.count += 1; }
    }

    fn last(&self) -> Option<&str> {
        if self.count == 0 { return None; }
        let idx = if self.head == 0 { HISTORY_SIZE - 1 } else { self.head - 1 };
        let n = self.lens[idx];
        core::str::from_utf8(&self.entries[idx][..n]).ok()
    }

    fn iter_recent(&self, max: usize) -> alloc::vec::Vec<(usize, &str)> {
        let mut result = alloc::vec::Vec::new();
        let show = max.min(self.count);
        for i in 0..show {
            let idx = if self.head >= show - i {
                self.head - show + i
            } else {
                HISTORY_SIZE - (show - i - self.head)
            };
            let n = self.lens[idx];
            if let Ok(s) = core::str::from_utf8(&self.entries[idx][..n]) {
                result.push((self.count - show + i + 1, s));
            }
        }
        result
    }
}

/// Parse and execute an intent command
pub fn execute(input: &str) {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return;
    }

    // !! = repeat last command
    if trimmed == "!!" {
        let hist = HISTORY.lock();
        if let Some(last) = hist.last() {
            let cmd = alloc::string::String::from(last);
            drop(hist);
            serial_println!("  → {}", cmd);
            execute(&cmd);
            return;
        } else {
            serial_println!("  No previous command");
            return;
        }
    }

    // Record in history (skip !! and history itself)
    if trimmed != "history" && !trimmed.starts_with("history ") {
        HISTORY.lock().push(trimmed);
    }

    // Split into command + args
    let mut parts = trimmed.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let args = parts.next().unwrap_or("").trim();
    // Check alias FIRST — resolve and re-execute
    {
        let aliases = ALIASES.lock();
        if let Some(expansion) = aliases.resolve(cmd) {
            let full = if args.is_empty() {
                alloc::string::String::from(expansion)
            } else {
                alloc::format!("{} {}", expansion, args)
            };
            drop(aliases);
            serial_println!("  → {}", full);
            execute(&full);
            return;
        }
    }

    // Try exact command match first
    let handled = match cmd {
        "help" | "?" => { cmd_help(); true },
        "sysinfo" => { cmd_sysinfo(); true },
        "status" | "s" => { cmd_status(); true },
        "ps" => { cmd_ps(); true },
        "agents" | "a" => { cmd_agents(); true },
        "caps" | "c" => { cmd_caps(); true },
        "audit" => { cmd_audit(args); true },
        "deploy" | "d" => { cmd_deploy(args); true },
        "pipe" | "p" => { cmd_pipe(args); true },
        "send" => { cmd_send(args); true },
        "tick" => { cmd_tick(); true },
        "ls" => { cmd_ls(); true },
        "cat" => { cmd_cat(args); true },
        "write" | "w" => { cmd_write(args); true },
        "rm" => { cmd_rm(args); true },
        "touch" => { cmd_touch(args); true },
        "mem" | "m" => { cmd_mem(); true },
        "kill" => { cmd_kill(args); true },
        "top" | "t" => { cmd_top(); true },
        "gc" => { cmd_gc(); true },
        "uptime" | "u" => { cmd_uptime(); true },
        "dmesg" => { cmd_dmesg(args); true },
        "set" => { cmd_set(args); true },
        "get" => { cmd_get(args); true },
        "unset" => { cmd_unset(args); true },
        "env" | "e" => { cmd_env(); true },
        "run" | "r" => { cmd_run(args); true },
        "alias" => { cmd_alias(args); true },
        "unalias" => { cmd_unalias(args); true },
        "aliases" => { cmd_aliases(); true },
        "history" | "h" => { cmd_history(args); true },
        "ask" => { cmd_ask(args); true },
        "clear" => { cmd_clear(); true },
        "about" => { cmd_about(); true },
        _ => false,
    };

    if !handled {
        // Fuzzy natural language matching
        fuzzy_intent(trimmed);
    }
}

/// Natural language intent matching
///
/// Maps keyword patterns to commands. Supports Swedish + English.
fn fuzzy_intent(input: &str) {
    // Normalize: lowercase for matching
    let mut buf = [0u8; 128];
    let n = input.len().min(127);
    buf[..n].copy_from_slice(&input.as_bytes()[..n]);
    // Manual ASCII lowercase
    for b in buf[..n].iter_mut() {
        if *b >= b'A' && *b <= b'Z' {
            *b += 32;
        }
    }
    let lower = core::str::from_utf8(&buf[..n]).unwrap_or("");

    // Extract potential argument (last word that looks like a name/number)
    let last_word = input.split_whitespace().last().unwrap_or("");

    // --- System queries (check BEFORE file ops to avoid false matches) ---
    if contains_any(lower, &["process", "vilka kor", "vad kor", "what's running", "whats running"]) {
        cmd_ps();
        return;
    }

    if contains_any(lower, &["minne", "memory", "ram", "hur mycket minne"]) {
        cmd_mem();
        return;
    }

    if contains_any(lower, &["status", "overview", "dashboard"]) {
        cmd_top();
        return;
    }

    if contains_any(lower, &["uptime", "hur lange", "how long", "tid"]) {
        cmd_uptime();
        return;
    }

    if contains_any(lower, &["kernel log", "boot log", "system log", "logg"]) {
        cmd_dmesg("20");
        return;
    }

    if contains_any(lower, &["capabilities", "rattigheter", "behorighe"]) {
        cmd_caps();
        return;
    }

    if contains_any(lower, &["audit", "gransk", "historik"]) {
        cmd_audit("5");
        return;
    }

    if contains_any(lower, &["clean", "rensa", "garbage", "stada"]) {
        cmd_gc();
        return;
    }

    if contains_any(lower, &["help", "hjalp", "kommandon", "commands", "vad kan"]) {
        cmd_help();
        return;
    }

    if contains_any(lower, &["about", "version", "info", "vad ar du", "vad ar tunicore"]) {
        cmd_about();
        return;
    }

    if contains_any(lower, &["environment", "variabler", "env var", "config", "konfigura", "installningar"]) {
        cmd_env();
        return;
    }

    // --- File operations ---
    if contains_any(lower, &["visa filer", "show file", "list file", "visa alla filer", "vilka filer"]) {
        cmd_ls();
        return;
    }

    // "visa <file>" / "read <file>" / "show <file>"
    if contains_any(lower, &["visa ", "read ", "show "]) {
        cmd_cat(last_word);
        return;
    }

    // "ta bort <file>" / "delete <file>" / "radera <file>"
    if contains_any(lower, &["ta bort", "delete", "radera", "remove"]) {
        cmd_rm(last_word);
        return;
    }


    // --- Agent operations ---
    if contains_any(lower, &["deploy ", "starta ", "start "]) ||
       (contains_any(lower, &["agent"]) && contains_any(lower, &["run", "start", "launch"])) {
        cmd_deploy(last_word);
        return;
    }

    if contains_any(lower, &["kill ", "terminate "]) ||
       contains_any(lower, &["stang av", "avsluta"]) {
        cmd_kill(last_word);
        return;
    }

    // Smart suggestion — find closest command
    let first = input.split_whitespace().next().unwrap_or("");
    if let Some(suggestion) = suggest_command(first) {
        serial_println!("  Unknown: '{}'. Did you mean '{}'?", first, suggestion);
    } else {
        // LLM fallback — ask AI if no command matched
        serial_println!("  \u{1F914} Thinking...");
        match crate::llm::query(input) {
            Ok(response) => {
                serial_println!("  {}", response);
            }
            Err(_e) => {
                serial_println!("  '{}' — I don't understand. Try 'help' or 'ask <question>'.", input);
            }
        }
    }
}

/// Check if haystack contains any of the needles
fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    for needle in needles {
        if haystack.contains(needle) {
            return true;
        }
    }
    false
}

/// Simple edit distance (Levenshtein) for short strings
fn edit_distance(a: &[u8], b: &[u8]) -> usize {
    let (m, n) = (a.len(), b.len());
    if m == 0 { return n; }
    if n == 0 { return m; }
    if n >= 31 { return n; }

    let mut prev = [0usize; 32];
    let mut curr = [0usize; 32];
    for j in 0..=n { prev[j] = j; }

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i-1] == b[j-1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j-1] + 1).min(prev[j-1] + cost);
        }
        for j in 0..=n { prev[j] = curr[j]; }
    }
    curr[n]
}

/// Suggest closest known command
fn suggest_command(input: &str) -> Option<&'static str> {
    const COMMANDS: &[&str] = &[
        "help", "status", "ps", "agents", "caps", "audit",
        "deploy", "pipe", "send", "tick", "ls", "cat",
        "write", "rm", "touch", "mem", "kill", "top",
        "gc", "uptime", "dmesg", "set", "get", "unset",
        "env", "run", "alias", "unalias", "aliases",
        "history", "clear", "about",
    ];

    let mut best: Option<&str> = None;
    let mut best_dist = usize::MAX;
    let input_bytes = input.as_bytes();

    for &cmd in COMMANDS {
        let d = edit_distance(input_bytes, cmd.as_bytes());
        if d < best_dist {
            best_dist = d;
            best = Some(cmd);
        }
    }

    if best_dist <= 2 && best_dist > 0 { best } else { None }
}

fn cmd_help() {
    serial_println!("  TuniCore Intent Commands:");
    serial_println!("  ─────────────────────────");
    serial_println!("  status (s)      System overview");
    serial_println!("  ps              Process table");
    serial_println!("  agents (a)      Active agent count");
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
    serial_println!("  mem  (m)        Physical memory stats");
    serial_println!("  kill <pid>      Terminate agent");
    serial_println!("  top  (t)        System dashboard");
    serial_println!("  gc              Clean dead processes");
    serial_println!("  uptime (u)      Time since boot");
    serial_println!("  dmesg [n]       Kernel message log");
    serial_println!("  set <k> <v>     Set environment var");
    serial_println!("  get <key>       Get environment var");
    serial_println!("  unset <key>     Remove environment var");
    serial_println!("  env  (e)        List all env vars");
    serial_println!("  run  (r) <a..>  Run agent workflow");
    serial_println!("  alias <n> <cmd> Define command alias");
    serial_println!("  unalias <name>  Remove alias");
    serial_println!("  aliases         List all aliases");
    serial_println!("  history (h) [n] Command history");
    serial_println!("  !!              Repeat last command");
    serial_println!("  sysinfo         System identity card");
    serial_println!("  ask <question>  Ask TuniCore AI");
    serial_println!("  clear           Clear screen");
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
    let active = table.active_count();
    let total = table.total_spawned();
    serial_println!("  Active: {}  Total spawned: {}", active, total);
}

fn cmd_ps() {
    use crate::agent::AgentState;

    let table = AGENT_TABLE.lock();
    let tick = interrupts::ticks();

    serial_println!("  {:>5} {:>7} {:16} {:>5} {:>8}", "PID", "STATE", "NAME", "CAPS", "AGE");
    serial_println!("  {:>5} {:>7} {:16} {:>5} {:>8}", "───", "─────", "────", "────", "───");

    let mut count = 0;
    for agent in table.iter() {
        let state = match agent.state {
            AgentState::Initializing => "INIT",
            AgentState::Active => "RUN",
            AgentState::Blocked => "BLOCK",
            AgentState::Suspended => "SUSP",
            AgentState::Terminated => "DEAD",
        };
        let age = tick.saturating_sub(agent.spawn_tick);
        serial_println!("  {:>5} {:>7} {:16} {:>5} {:>8}",
            agent.id.0, state, agent.name_str(),
            agent.capabilities.len(), age);
        count += 1;
    }
    serial_println!("  {} processes", count);
}

fn cmd_kill(args: &str) {
    if args.is_empty() {
        serial_println!("  Usage: kill <pid>");
        return;
    }
    let pid: u32 = match args.parse() {
        Ok(v) => v,
        Err(_) => {
            serial_println!("  Invalid PID: '{}'", args);
            return;
        }
    };

    let mut table = AGENT_TABLE.lock();
    let id = crate::cap_table::AgentId(pid);
    match table.kill(id) {
        Ok(()) => serial_println!("  Killed agent:{}", pid),
        Err(e) => serial_println!("  Kill failed: {}", e),
    }
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
        serial_println!("  Available: hello, sender, receiver, writer, analyzer, greeter");
        return;
    }

    serial_println!("  Deploying '{}'...", name);

    // Use the built-in WASM agents
    static HELLO_WASM: &[u8] = include_bytes!("hello_agent.wasm");
    static SENDER_WASM: &[u8] = include_bytes!("sender_agent.wasm");
    static RECEIVER_WASM: &[u8] = include_bytes!("receiver_agent.wasm");
    static WRITER_WASM: &[u8] = include_bytes!("writer_agent.wasm");
    static ANALYZER_WASM: &[u8] = include_bytes!("analyzer_agent.wasm");
    static GREETER_WASM: &[u8] = include_bytes!("greeter_agent.wasm");

    let (wasm, chan_w, chan_r) = match name {
        "hello" => (HELLO_WASM, None, None),
        "writer" => (WRITER_WASM, None, None),
        "analyzer" => (ANALYZER_WASM, None, None),
        "greeter" => (GREETER_WASM, None, None),
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

fn cmd_mem() {
    let stats = crate::memory::page_alloc::stats();
    serial_println!("  ┌─────────────────────────────┐");
    serial_println!("  │ Physical Memory              │");
    serial_println!("  ├─────────────────────────────┤");
    serial_println!("  │ Total:   {:>5} MiB ({:>7}) │", stats.total_mb(), stats.total);
    serial_println!("  │ Used:    {:>5} MiB ({:>7}) │", stats.used_mb(), stats.used);
    serial_println!("  │ Free:    {:>5} MiB ({:>7}) │", stats.free_mb(), stats.free);
    serial_println!("  │ Page:    4 KiB               │");
    serial_println!("  └─────────────────────────────┘");

    let fs = FS.lock();
    serial_println!("  VirtFS: {} files, {} bytes", fs.file_count(), fs.total_size());
    serial_println!("  Heap:   32 MiB (static)");
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
    static ANALYZER_WASM: &[u8] = include_bytes!("analyzer_agent.wasm");
    static GREETER_WASM: &[u8] = include_bytes!("greeter_agent.wasm");

    let wasm_a = match agent_a {
        "hello" => Some(HELLO_WASM),
        "sender" => Some(SENDER_WASM),
        "writer" => Some(WRITER_WASM),
        "analyzer" => Some(ANALYZER_WASM),
        "greeter" => Some(GREETER_WASM),
        _ => None,
    };

    let wasm_b = match agent_b {
        "hello" => Some(HELLO_WASM),
        "receiver" => Some(RECEIVER_WASM),
        "writer" => Some(WRITER_WASM),
        "analyzer" => Some(ANALYZER_WASM),
        "greeter" => Some(GREETER_WASM),
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

fn cmd_top() {
    use crate::agent::AgentState;

    let tick = interrupts::ticks();
    let secs = tick / 120; // rough: ~120 ticks/sec at our APIC rate
    let agents_table = AGENT_TABLE.lock();
    let active = agents_table.active_count();
    let total = agents_table.total_spawned();
    let mut dead = 0u32;
    for a in agents_table.iter() {
        if a.state == AgentState::Terminated { dead += 1; }
    }
    drop(agents_table);

    let caps = CAP_TABLE.lock().active_count();
    let audit = AUDIT_LOG.lock().total_events();
    let mem = crate::memory::page_alloc::stats();
    let fs = FS.lock();
    let files = fs.file_count();
    let fs_bytes = fs.total_size();
    drop(fs);

    serial_println!("  ══════ TuniCore v0.5.0 ══════");
    serial_println!("  Uptime: ~{}s   Tick: {}", secs, tick);
    serial_println!("  CPU:    x86_64 (1 core)");
    serial_println!("  ─────────────────────────────");
    serial_println!("  PROCS   {} run, {} dead, {} total", active, dead, total);
    serial_println!("  RAM     {} MiB total, {} MiB free", mem.total_mb(), mem.free_mb());
    serial_println!("  HEAP    32 MiB static");
    serial_println!("  FS      {} files ({} B)", files, fs_bytes);
    serial_println!("  CAPS    {} active / 4096 max", caps);
    serial_println!("  AUDIT   {} events", audit);
    serial_println!("  ═════════════════════════════");
}

fn cmd_gc() {
    let before = {
        let table = AGENT_TABLE.lock();
        let mut dead = 0usize;
        for a in table.iter() {
            if a.state == crate::agent::AgentState::Terminated { dead += 1; }
        }
        dead
    };
    AGENT_TABLE.lock().gc();
    serial_println!("  Cleaned {} dead processes", before);
}

fn cmd_uptime() {
    let tick = interrupts::ticks();
    let secs = tick / 120;
    let mins = secs / 60;
    serial_println!("  Uptime: ~{}m {}s ({} ticks)", mins, secs % 60, tick);
}

fn cmd_set(args: &str) {
    let mut parts = args.splitn(2, ' ');
    let key = parts.next().unwrap_or("");
    let val = parts.next().unwrap_or("");
    if key.is_empty() {
        serial_println!("  Usage: set <key> <value>");
        return;
    }
    match ENV.lock().set(key, val) {
        Ok(()) => serial_println!("  {} = {}", key, val),
        Err(e) => serial_println!("  Error: {}", e),
    }
}

fn cmd_get(args: &str) {
    if args.is_empty() {
        serial_println!("  Usage: get <key>");
        return;
    }
    match ENV.lock().get(args) {
        Some(val) => serial_println!("  {} = {}", args, val),
        None => serial_println!("  '{}' not set", args),
    }
}

fn cmd_unset(args: &str) {
    if args.is_empty() {
        serial_println!("  Usage: unset <key>");
        return;
    }
    if ENV.lock().unset(args) {
        serial_println!("  Removed '{}'", args);
    } else {
        serial_println!("  '{}' not found", args);
    }
}

fn cmd_env() {
    let env = ENV.lock();
    serial_println!("  Environment ({} vars):", env.len());
    serial_println!("  {:16} {}", "KEY", "VALUE");
    serial_println!("  {:16} {}", "───", "─────");
    for (k, v) in env.iter() {
        serial_println!("  {:16} {}", k, v);
    }
}

fn cmd_run(args: &str) {
    if args.is_empty() {
        serial_println!("  Usage: run <agent1> <agent2> ...");
        serial_println!("  Example: run writer analyzer");
        return;
    }

    let agents: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    let total = agents.len();
    serial_println!("  ─── Workflow: {} agents ───", total);

    static HELLO_WASM: &[u8] = include_bytes!("hello_agent.wasm");
    static WRITER_WASM: &[u8] = include_bytes!("writer_agent.wasm");
    static ANALYZER_WASM: &[u8] = include_bytes!("analyzer_agent.wasm");
    static GREETER_WASM: &[u8] = include_bytes!("greeter_agent.wasm");

    let mut ok = 0u32;
    let mut fail = 0u32;

    for (i, name) in agents.iter().enumerate() {
        serial_println!("  [{}/{}] {}...", i + 1, total, name);

        let wasm: Option<&[u8]> = match *name {
            "hello" => Some(HELLO_WASM),
            "writer" => Some(WRITER_WASM),
            "analyzer" => Some(ANALYZER_WASM),
            "greeter" => Some(GREETER_WASM),
            _ => None,
        };

        match wasm {
            Some(bytes) => {
                match crate::wasm_runtime::execute_agent(name, bytes, None, None, None) {
                    Ok(()) => {
                        serial_println!("  [{}/{}] {} ✓", i + 1, total, name);
                        ok += 1;
                    }
                    Err(e) => {
                        serial_println!("  [{}/{}] {} ✗ ({})", i + 1, total, name, e);
                        fail += 1;
                    }
                }
            }
            None => {
                serial_println!("  [{}/{}] {} ✗ (unknown agent)", i + 1, total, name);
                fail += 1;
            }
        }
    }

    serial_println!("  ─── Workflow complete: {} ok, {} failed ───", ok, fail);
}

fn cmd_alias(args: &str) {
    let mut parts = args.splitn(2, ' ');
    let name = parts.next().unwrap_or("");
    let expansion = parts.next().unwrap_or("");
    if name.is_empty() || expansion.is_empty() {
        serial_println!("  Usage: alias <name> <command>");
        serial_println!("  Example: alias report run writer analyzer");
        return;
    }
    match ALIASES.lock().define(name, expansion) {
        Ok(()) => serial_println!("  Alias '{}' → '{}'", name, expansion),
        Err(e) => serial_println!("  Error: {}", e),
    }
}

fn cmd_unalias(args: &str) {
    if args.is_empty() {
        serial_println!("  Usage: unalias <name>");
        return;
    }
    if ALIASES.lock().remove(args) {
        serial_println!("  Removed alias '{}'", args);
    } else {
        serial_println!("  Alias '{}' not found", args);
    }
}

fn cmd_aliases() {
    let table = ALIASES.lock();
    if table.len() == 0 {
        serial_println!("  No aliases defined. Use: alias <name> <command>");
        return;
    }
    serial_println!("  Aliases ({}):", table.len());
    serial_println!("  {:12} {}", "NAME", "EXPANDS TO");
    serial_println!("  {:12} {}", "────", "──────────");
    for (name, expansion) in table.iter() {
        serial_println!("  {:12} {}", name, expansion);
    }
}

fn cmd_history(args: &str) {
    let n: usize = args.parse().unwrap_or(10);
    let hist = HISTORY.lock();
    let entries = hist.iter_recent(n);
    if entries.is_empty() {
        serial_println!("  No command history yet.");
        return;
    }
    serial_println!("  History (last {}):", entries.len());
    for (num, cmd) in entries {
        serial_println!("  {:4}  {}", num, cmd);
    }
}

fn cmd_sysinfo() {
    let tick = interrupts::ticks();
    let secs = tick / 100;

    // ASCII art logo
    serial_println!();
    serial_println!("  ████████╗ ██████╗");
    serial_println!("  ╚══██╔══╝██╔════╝");
    serial_println!("     ██║   ██║");
    serial_println!("     ██║   ██║");
    serial_println!("     ██║   ╚██████╗");
    serial_println!("     ╚═╝    ╚═════╝");
    serial_println!();

    // System identity — copy values before dropping lock
    let (hostname, version, owner, lang, shell, env_count) = {
        let env = ENV.lock();
        let h = alloc::string::String::from(env.get("hostname").unwrap_or("unknown"));
        let v = alloc::string::String::from(env.get("version").unwrap_or("?"));
        let o = alloc::string::String::from(env.get("owner").unwrap_or("-"));
        let l = alloc::string::String::from(env.get("lang").unwrap_or("en"));
        let s = alloc::string::String::from(env.get("shell").unwrap_or("?"));
        let c = env.len();
        (h, v, o, l, s, c)
    };

    serial_println!("  OS        TuniCore v{}", version);
    serial_println!("  Host      {}", hostname);
    serial_println!("  Owner     {}", owner);
    serial_println!("  Arch      x86_64");
    serial_println!("  Uptime    ~{}m {}s ({} ticks)", secs / 60, secs % 60, tick);
    serial_println!("  Shell     {}", shell);
    serial_println!("  Lang      {}", lang);
    serial_println!();

    // Hardware
    let stats = crate::memory::page_alloc::stats();
    serial_println!("  RAM       {}/{} MiB free", stats.free_mb(), stats.total_mb());
    serial_println!("  Heap      32 MiB static");

    // Filesystem
    let fs = FS.lock();
    let files = fs.file_count();
    let bytes = fs.total_size();
    drop(fs);
    serial_println!("  Files     {} ({} B)", files, bytes);

    // Processes
    let table = AGENT_TABLE.lock();
    let active = table.active_count();
    let total_spawned = table.total_spawned();
    drop(table);
    serial_println!("  Procs     {} active, {} total spawned", active, total_spawned);

    // Capabilities
    let caps = CAP_TABLE.lock().active_count();
    serial_println!("  Caps      {}/4096 active", caps);

    // Audit
    let audit = AUDIT_LOG.lock().total_events();
    serial_println!("  Audit     {} events", audit);

    // Environment & aliases
    let alias_count = ALIASES.lock().len();
    let hist_count = HISTORY.lock().count;
    serial_println!("  Env       {} vars", env_count);
    serial_println!("  Aliases   {}", alias_count);
    serial_println!("  History   {} entries", hist_count);
    serial_println!("  Agents    6 built-in (hello, writer, analyzer, greeter, sender, receiver)");
    serial_println!("  Commands  36 exact + NLP fuzzy");
    serial_println!();
}

fn cmd_ask(args: &str) {
    if args.is_empty() {
        serial_println!("  Usage: ask <question>");
        serial_println!("  Example: ask vad är Rust?");
        return;
    }
    serial_println!("  \u{1F914} Thinking...");
    match crate::llm::query(args) {
        Ok(response) => {
            serial_println!("  {}", response);
        }
        Err(e) => {
            serial_println!("  AI unavailable: {}", e);
            serial_println!("  Start the bridge: python3 tools/llm_bridge.py");
        }
    }
}

fn cmd_clear() {
    serial_println!("\x1B[2J\x1B[H");
}

fn cmd_dmesg(args: &str) {
    let n: usize = args.parse().unwrap_or(20);
    let klog = crate::klog::KLOG.lock();
    serial_println!("  Kernel log ({} total, showing last {}):", klog.total(), n);
    serial_println!("  {:>6} {:>4} {}", "TICK", "LVL", "MESSAGE");
    serial_println!("  {:>6} {:>4} {}", "────", "───", "───────");
    for entry in klog.recent(n) {
        if entry.valid {
            serial_println!("  {:>6} {:>4} {}",
                entry.tick, entry.level.tag(), entry.message());
        }
    }
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
