//! Resource abstraction for TuniCore
//!
//! Every resource in the system is capability-gated.
//! Resources are the "what" that capabilities control access to.
//! This is fundamentally different from Unix where resources are files —
//! here, resources are typed and include compute, memory, channels, and devices.

/// Unique resource identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceId(pub u64);

/// Unique device identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceId(pub u16);

/// A reference to a specific resource in the system
#[derive(Debug, Clone)]
pub enum ResourceRef {
    /// A memory region (base + length in HHDM space)
    Memory {
        base: u64,
        length: u64,
    },

    /// A hardware I/O port range
    IoPort {
        base: u16,
        count: u16,
    },

    /// A compute slot (CPU time, GPU queue, NPU unit)
    Compute {
        device: DeviceId,
        slot: u16,
    },

    /// The serial debug console
    Serial,

    /// A framebuffer region for display output
    Display {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },

    /// An inter-agent communication channel
    Channel {
        channel_id: u64,
    },

    /// The kernel audit log (read-only for agents)
    AuditLog,
}

impl ResourceRef {
    /// Get a human-readable description (for audit logs)
    pub fn describe(&self) -> &'static str {
        match self {
            ResourceRef::Memory { .. } => "memory",
            ResourceRef::IoPort { .. } => "io_port",
            ResourceRef::Compute { .. } => "compute",
            ResourceRef::Serial => "serial",
            ResourceRef::Display { .. } => "display",
            ResourceRef::Channel { .. } => "channel",
            ResourceRef::AuditLog => "audit_log",
        }
    }
}
