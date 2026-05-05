# TuniCore

> **The world's first conversational operating system.**
> No GUI. No terminal commands to memorize. Just talk to it.

https://github.com/hencyber/tunicore/raw/main/docs/demo.mp4

TuniCore is a bare-metal x86_64 operating system written in Rust where the primary interface is natural language. Instead of memorizing `ls -la`, you say *"show my files"*. Instead of scripting in Bash, you teach it with *"alias report run writer analyzer"*. And when it doesn't understand? It asks AI.

```
tc> show configuration
  hostname = tunicore
  version  = 0.6.0
  owner    = tuncore
  lang     = en

tc> deploy greeter
  [agent:5] env_get('hostname') = tunicore
  [agent:5] env_get('owner') = tuncore
  [agent:5] Wrote 80 bytes to 'greeting.md'
  Greeting generated!

tc> deply hello
  Unknown: 'deply'. Did you mean 'deploy'?

tc> ask what is Rust?
  Thinking...
  Rust is a systems programming language designed for safety and performance...
```

---

## What Makes This Different

Every hobby OS copies Unix. TuniCore doesn't.

| Traditional OS | TuniCore |
|---|---|
| Commands you memorize | Say what you want in natural language |
| Bash scripts | Teach aliases: `alias report run writer analyzer` |
| `man pages` | Typo? "Did you mean 'deploy'?" (Levenshtein) |
| Processes | WASM agents with capability-based sandboxing |
| `/etc/` config files | `set hostname tunicore` - live, in-memory |
| Terminal only | AI-powered: `ask` anything, get answers from Gemini |

## Architecture

```
+-------------------------------------------------+
|  LLM Bridge (Python - Gemini/OpenAI)            |
|  Serial <-> AI API                              |
+------------------+------------------------------+
|  NLP Intent      |  WASM Agent Runtime           |
|  EN+SE parsing   |  wasmi interpreter            |
|  Levenshtein     |  8 host functions              |
|  Smart suggest   |  6 built-in agents             |
+------------------+------------------------------+
|  Kernel Services                                 |
|  VirtFS - Channels - Env Store - Aliases         |
|  Audit Trail - Command History - Klog            |
+-------------------------------------------------+
|  Security Layer                                  |
|  Capability Table - Resource Budgets - Guardian   |
+-------------------------------------------------+
|  Hardware Abstraction                            |
|  x86_64 - GDT - IDT - PIC - UART - PMM - Heap  |
+-------------------------------------------------+
```

## Features

### Conversational Shell (37 commands + NLP)
- **NLP: English natural language parsing (Swedish also supported)
- **Smart Suggestions**: Typo -> Levenshtein edit distance -> "Did you mean...?"
- **Command History**: Ring buffer with `!!` repeat
- **User Aliases**: `alias deploy-all run writer analyzer greeter`

### WASM Agent Runtime
- **6 built-in agents**: hello, writer, analyzer, greeter, sender, receiver
- **8 host functions**: `tc.log`, `tc.time`, `tc.fs_write`, `tc.fs_read`, `tc.chan_send`, `tc.chan_recv`, `tc.env_get`
- **Workflow orchestration**: `run writer analyzer` - sequential agent pipelines
- **Pipe mode**: `pipe sender receiver` - channel-based IPC

### Security
- **Capability-based access control** - no ambient authority
- **Resource budgets** - CPU, memory, I/O limits per agent
- **Agent timeouts** - auto-kill runaway processes
- **Full audit trail** - every action logged with tick-level timestamps

### AI Integration
- **`ask` command** - query Gemini or OpenAI from bare metal
- **LLM fallback** - unrecognized input -> AI instead of error
- **Serial bridge protocol** - `STX+LLM:query+ETX` <-> `STX+RSP:response+ETX`

### System Identity (`sysinfo`)
```
  OS        TuniCore v0.6.0
  Host      tunicore
  Owner     tuncore
  Arch      x86_64
  Uptime    ~0m 5s
  Shell     intent/v2
  Lang      en
  RAM       401/402 MiB free
  Agents    6 built-in
  Commands  37 exact + NLP fuzzy
```

## Quick Start

### Prerequisites

```bash
# Rust nightly
rustup default nightly
rustup target add x86_64-unknown-none
rustup component add rust-src

# System tools
sudo apt install qemu-system-x86_64 xorriso make
```

### Build and Run

```bash
make              # Build ISO
make run-uefi     # Run in QEMU (UEFI)
```

### With AI (optional)

```bash
export GEMINI_API_KEY="your-key"
python3 tools/llm_bridge.py    # In a separate terminal
# Then boot TuniCore - 'ask' commands will get AI responses
```

## Project Structure

```
kernel/src/
  main.rs           # Boot sequence + shell REPL
  intent.rs         # NLP command dispatcher (37 commands)
  llm.rs            # Serial-based LLM bridge protocol
  wasm_runtime.rs   # WASM interpreter + 8 host functions
  agent.rs          # Process table + resource budgets
  virtfs.rs         # In-memory filesystem (64 files)
  channel.rs        # IPC channels for agent communication
  env.rs            # Key-value environment store
  alias.rs          # User-defined command aliases
  cap_table.rs      # Capability-based access control
  guardian.rs       # Security policy enforcement
  audit.rs          # Tamper-proof audit trail
  klog.rs           # Kernel ring buffer logger
  serial.rs         # UART 16550 driver
  interrupts.rs     # PIC + timer (100Hz tick)
  memory/           # PMM + heap allocator
  *.wasm            # 6 built-in WASM agents
tools/
  llm_bridge.py     # AI bridge (Gemini/OpenAI)
```

## Development Timeline

| Phase | Feature | Status |
|-------|---------|--------|
| 1-4 | Boot, GDT, IDT, Heap, Serial | Done |
| 5-7 | Capability system, Agent table, Audit | Done |
| 8-10 | WASM runtime, VirtFS, Intent parser | Done |
| 11-13 | NLP (EN+SE), Hardware detect, Agent I/O | Done |
| 14-15 | IPC channels, Pipe orchestration | Done |
| 16-18 | Env store, Workflows, Aliases | Done |
| 19-20 | Agent config access, Command history | Done |
| 21-22 | Smart suggestions, Sysinfo | Done |
| 23 | LLM Bridge (AI integration) | Done |

23 phases. 37 commands. 6 agents. 8 host functions. 28 source files. 1 person.

## FAQ

**Q: Is the NLP "real"?**
A: It's rule-based keyword matching + Levenshtein fuzzy matching + LLM fallback via serial bridge. Not a local neural network, but the architecture is designed so that when on-device LLMs become feasible on bare metal, the intent parser can be swapped.

**Q: Why not just use Linux?**
A: Because TuniCore isn't trying to be Linux. It's exploring what an OS could be if we skipped GUIs entirely and went straight from terminal to conversation.

**Q: Can I write my own agents?**
A: Yes. Any WASM binary that imports `tc.*` host functions can be deployed. See the `.wasm` files in `kernel/src/` for examples.

## License

MIT

---

*Built with Rust, no standard library, on bare metal.*
*TuniCore - the OS you talk to.*
