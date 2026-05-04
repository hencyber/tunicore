//! Capability system for TuniCore
//!
//! This module defines the type-safe capability model that will govern
//! all resource access in the kernel. In an agent-first OS, capabilities
//! replace traditional Unix permissions: instead of "user X can read file Y",
//! we have "agent A holds capability C which grants READ on resource R".
//!
//! ## Design Principles
//!
//! 1. **Principle of Least Privilege**: Agents start with zero capabilities
//!    and must be explicitly granted each one.
//!
//! 2. **Non-forgeable**: Capabilities are kernel-managed tokens, not
//!    user-space values that can be guessed or fabricated.
//!
//! 3. **Delegatable with attenuation**: A capability holder can grant
//!    a subset of their rights to a sub-agent, but never escalate.
//!
//! 4. **Revocable**: The kernel can revoke any capability at any time,
//!    enabling kill-switch behavior for misbehaving agents.
//!
//! ## Phase 1 Status
//!
//! Type definitions only. No enforcement. The capability types exist
//! so that future syscalls can be designed around them from the start.

pub mod types;

pub use types::*;

/// Initialize the capability subsystem
pub fn init() {
    // Phase 1: nothing to initialize, types are compile-time only.
    // Phase 2: will set up the capability table and root capability set.
}
