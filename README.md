# TuniCore

> **A capability-based, Rust-native kernel for the AI agent era.**

TuniCore is an experimental operating system kernel designed from the ground up to run AI agents securely. Instead of the traditional user/group/permission model, TuniCore uses a **capability-based security architecture** where every agent receives explicit, attenuable, revocable tokens governing what it can do.

## Vision

The operating systems of tomorrow won't look like today's. Users will talk to agents, not click through GUIs. But underneath that agent, something needs to:

- **Sandbox** the code the agent generates and runs
- **Control** what resources the agent can access
- **Audit** every action the agent takes
- **Kill** a misbehaving agent instantly and safely

That's TuniCore. **The agent is the interface. The kernel is the guard.**

## Architecture

```
┌─────────────────────────────────────────┐
│  Agent Layer (future)                   │
│  Natural language → actions             │
├─────────────────────────────────────────┤
│  Capability Gate                        │
│  Every syscall checked against caps     │
├─────────────────────────────────────────┤
│  TuniCore Kernel (Rust, no_std)         │
│  GDT · IDT · Heap · Serial · FB        │
├─────────────────────────────────────────┤
│  HAL (unsafe boundary)                  │
│  x86_64 ports · page tables · PIC/APIC │
└─────────────────────────────────────────┘
```

## Building

### Prerequisites

- Rust nightly (`rustup default nightly`)
- `x86_64-unknown-none` target (`rustup target add x86_64-unknown-none`)
- `rust-src` component (`rustup component add rust-src`)
- QEMU (`qemu-system-x86_64`)
- `xorriso` (for ISO creation)
- GNU Make

### Build & Run

```bash
make          # Build ISO
make run      # Run in QEMU (BIOS, serial → terminal)
make run-uefi # Run in QEMU with UEFI firmware
make clean    # Clean build artifacts
```

## Project Status

**Phase 1** — Bootable kernel with:
- [x] Limine bootloader integration
- [x] Serial console (UART 16550)
- [x] Framebuffer rendering
- [x] GDT + TSS (with IST for double-fault)
- [x] IDT with exception handlers
- [x] PIC 8259 hardware interrupts
- [x] Heap allocator (1 MiB linked-list)
- [x] Capability type system (skeleton)

## License

MIT
