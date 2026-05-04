//! Hardware detection via CPUID
//!
//! Detects modern CPU features critical for TuniCore's security model:
//! - APIC/x2APIC for modern interrupt handling
//! - AMD SEV/SEV-SNP for confidential agent memory
//! - Intel TDX for trust domain isolation
//! - TSC Deadline for precision agent timeouts
//! - FSGSBASE for fast agent context switching

use crate::serial_println;

/// Detected hardware capabilities
#[derive(Debug)]
pub struct HwCaps {
    /// Local APIC present
    pub apic: bool,
    /// x2APIC mode supported (scalable APIC)
    pub x2apic: bool,
    /// TSC Deadline timer (precision timeouts)
    pub tsc_deadline: bool,
    /// FSGSBASE instructions (fast context switch)
    pub fsgsbase: bool,
    /// 1 GiB pages supported
    pub gigabyte_pages: bool,
    /// RDRAND instruction (hardware RNG)
    pub rdrand: bool,
    /// AMD SEV (Secure Encrypted Virtualization)
    pub sev: bool,
    /// AMD SEV-SNP (Secure Nested Paging)
    pub sev_snp: bool,
    /// Intel TME (Total Memory Encryption)
    pub tme: bool,
    /// SSE4.2 (for fast hashing in audit log)
    pub sse42: bool,
    /// CPU vendor string
    pub vendor: CpuVendor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuVendor {
    Intel,
    Amd,
    Unknown,
}

/// Execute CPUID instruction
#[inline]
fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let (eax, ebx, ecx, edx);
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            ebx_out = out(reg) ebx,
            out("ecx") ecx,
            out("edx") edx,
        );
    }
    (eax, ebx, ecx, edx)
}

/// Execute CPUID with sub-leaf
#[inline]
fn cpuid_sub(leaf: u32, sub: u32) -> (u32, u32, u32, u32) {
    let (eax, ebx, ecx, edx);
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            ebx_out = out(reg) ebx,
            inout("ecx") sub => ecx,
            out("edx") edx,
        );
    }
    (eax, ebx, ecx, edx)
}

/// Detect CPU vendor from CPUID leaf 0
fn detect_vendor() -> CpuVendor {
    let (_, ebx, ecx, edx) = cpuid(0);
    // "GenuineIntel" = ebx=756e6547 edx=49656e69 ecx=6c65746e
    // "AuthenticAMD" = ebx=68747541 edx=69746e65 ecx=444d4163
    if ebx == 0x756e6547 && edx == 0x49656e69 && ecx == 0x6c65746e {
        CpuVendor::Intel
    } else if ebx == 0x68747541 && edx == 0x69746e65 && ecx == 0x444d4163 {
        CpuVendor::Amd
    } else {
        CpuVendor::Unknown
    }
}

/// Detect all hardware capabilities via CPUID
pub fn detect() -> HwCaps {
    let vendor = detect_vendor();

    // Leaf 1: basic feature flags
    let (_, _, ecx1, edx1) = cpuid(1);
    let apic = edx1 & (1 << 9) != 0;
    let x2apic = ecx1 & (1 << 21) != 0;
    let tsc_deadline = ecx1 & (1 << 24) != 0;
    let sse42 = ecx1 & (1 << 20) != 0;
    let rdrand = ecx1 & (1 << 30) != 0;

    // Leaf 7: extended features
    let (max_leaf, _, _, _) = cpuid(0);
    let fsgsbase = if max_leaf >= 7 {
        let (_, ebx7, _, _) = cpuid_sub(7, 0);
        ebx7 & (1 << 0) != 0
    } else {
        false
    };

    // Extended leaf 0x80000001: AMD features + 1G pages
    let (max_ext, _, _, _) = cpuid(0x80000000);
    let gigabyte_pages = if max_ext >= 0x80000001 {
        let (_, _, _, edx_ext) = cpuid(0x80000001);
        edx_ext & (1 << 26) != 0
    } else {
        false
    };

    // AMD SEV detection (leaf 0x8000001F)
    let (sev, sev_snp) = if vendor == CpuVendor::Amd && max_ext >= 0x8000001F {
        let (eax_sev, _, _, _) = cpuid(0x8000001F);
        let sev = eax_sev & (1 << 1) != 0;
        let sev_snp = eax_sev & (1 << 4) != 0;
        (sev, sev_snp)
    } else {
        (false, false)
    };

    // Intel TME detection (CPUID leaf 7, ECX bit 13)
    let tme = if vendor == CpuVendor::Intel && max_leaf >= 7 {
        let (_, _, ecx7, _) = cpuid_sub(7, 0);
        ecx7 & (1 << 13) != 0
    } else {
        false
    };

    HwCaps {
        apic,
        x2apic,
        tsc_deadline,
        fsgsbase,
        gigabyte_pages,
        rdrand,
        sev,
        sev_snp,
        tme,
        sse42,
        vendor,
    }
}

/// Log detected capabilities to serial
pub fn log_capabilities(caps: &HwCaps) {
    serial_println!("[hwdetect] CPU: {:?}", caps.vendor);
    serial_println!("[hwdetect] APIC: {} | x2APIC: {}", yn(caps.apic), yn(caps.x2apic));
    serial_println!("[hwdetect] TSC Deadline: {} | FSGSBASE: {}", yn(caps.tsc_deadline), yn(caps.fsgsbase));
    serial_println!("[hwdetect] RDRAND: {} | SSE4.2: {}", yn(caps.rdrand), yn(caps.sse42));
    serial_println!("[hwdetect] 1GiB pages: {}", yn(caps.gigabyte_pages));

    // Confidential computing
    match caps.vendor {
        CpuVendor::Amd => {
            serial_println!("[hwdetect] AMD SEV: {} | SEV-SNP: {}", yn(caps.sev), yn(caps.sev_snp));
        }
        CpuVendor::Intel => {
            serial_println!("[hwdetect] Intel TME: {}", yn(caps.tme));
        }
        _ => {}
    }
}

fn yn(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}
