use crate::translator::{operand::SegmentReg, util::Width};

const GPR_NAMES: &[(&str, X64GpReg, X64GpSlice)] = &[
    ("rax", X64GpReg::Rax, X64GpSlice::Full), ("eax", X64GpReg::Rax, X64GpSlice::Low32),
    ("ax", X64GpReg::Rax, X64GpSlice::Low16), ("al", X64GpReg::Rax, X64GpSlice::Low8), ("ah", X64GpReg::Rax, X64GpSlice::High8),
 
    ("rbx", X64GpReg::Rbx, X64GpSlice::Full), ("ebx", X64GpReg::Rbx, X64GpSlice::Low32),
    ("bx", X64GpReg::Rbx, X64GpSlice::Low16), ("bl", X64GpReg::Rbx, X64GpSlice::Low8), ("bh", X64GpReg::Rbx, X64GpSlice::High8),
 
    ("rcx", X64GpReg::Rcx, X64GpSlice::Full), ("ecx", X64GpReg::Rcx, X64GpSlice::Low32),
    ("cx", X64GpReg::Rcx, X64GpSlice::Low16), ("cl", X64GpReg::Rcx, X64GpSlice::Low8), ("ch", X64GpReg::Rcx, X64GpSlice::High8),
 
    ("rdx", X64GpReg::Rdx, X64GpSlice::Full), ("edx", X64GpReg::Rdx, X64GpSlice::Low32),
    ("dx", X64GpReg::Rdx, X64GpSlice::Low16), ("dl", X64GpReg::Rdx, X64GpSlice::Low8), ("dh", X64GpReg::Rdx, X64GpSlice::High8),
 
    ("rsi", X64GpReg::Rsi, X64GpSlice::Full), ("esi", X64GpReg::Rsi, X64GpSlice::Low32),
    ("si", X64GpReg::Rsi, X64GpSlice::Low16), ("sil", X64GpReg::Rsi, X64GpSlice::Low8),
 
    ("rdi", X64GpReg::Rdi, X64GpSlice::Full), ("edi", X64GpReg::Rdi, X64GpSlice::Low32),
    ("di", X64GpReg::Rdi, X64GpSlice::Low16), ("dil", X64GpReg::Rdi, X64GpSlice::Low8),
 
    ("rbp", X64GpReg::Rbp, X64GpSlice::Full), ("ebp", X64GpReg::Rbp, X64GpSlice::Low32),
    ("bp", X64GpReg::Rbp, X64GpSlice::Low16), ("bpl", X64GpReg::Rbp, X64GpSlice::Low8),
 
    ("rsp", X64GpReg::Rsp, X64GpSlice::Full), ("esp", X64GpReg::Rsp, X64GpSlice::Low32),
    ("sp", X64GpReg::Rsp, X64GpSlice::Low16), ("spl", X64GpReg::Rsp, X64GpSlice::Low8),
 
    ("r8", X64GpReg::R8, X64GpSlice::Full), ("r8d", X64GpReg::R8, X64GpSlice::Low32), ("r8w", X64GpReg::R8, X64GpSlice::Low16), ("r8b", X64GpReg::R8, X64GpSlice::Low8),
    ("r9", X64GpReg::R9, X64GpSlice::Full), ("r9d", X64GpReg::R9, X64GpSlice::Low32), ("r9w", X64GpReg::R9, X64GpSlice::Low16), ("r9b", X64GpReg::R9, X64GpSlice::Low8),
    ("r10", X64GpReg::R10, X64GpSlice::Full), ("r10d", X64GpReg::R10, X64GpSlice::Low32), ("r10w", X64GpReg::R10, X64GpSlice::Low16), ("r10b", X64GpReg::R10, X64GpSlice::Low8),
    ("r11", X64GpReg::R11, X64GpSlice::Full), ("r11d", X64GpReg::R11, X64GpSlice::Low32), ("r11w", X64GpReg::R11, X64GpSlice::Low16), ("r11b", X64GpReg::R11, X64GpSlice::Low8),
    ("r12", X64GpReg::R12, X64GpSlice::Full), ("r12d", X64GpReg::R12, X64GpSlice::Low32), ("r12w", X64GpReg::R12, X64GpSlice::Low16), ("r12b", X64GpReg::R12, X64GpSlice::Low8),
    ("r13", X64GpReg::R13, X64GpSlice::Full), ("r13d", X64GpReg::R13, X64GpSlice::Low32), ("r13w", X64GpReg::R13, X64GpSlice::Low16), ("r13b", X64GpReg::R13, X64GpSlice::Low8),
    ("r14", X64GpReg::R14, X64GpSlice::Full), ("r14d", X64GpReg::R14, X64GpSlice::Low32), ("r14w", X64GpReg::R14, X64GpSlice::Low16), ("r14b", X64GpReg::R14, X64GpSlice::Low8),
    ("r15", X64GpReg::R15, X64GpSlice::Full), ("r15d", X64GpReg::R15, X64GpSlice::Low32), ("r15w", X64GpReg::R15, X64GpSlice::Low16), ("r15b", X64GpReg::R15, X64GpSlice::Low8),
];
 
/// Resolves an assembly-source identifier to its semantic register, or
/// `None` if it isn't a recognized register name (e.g. it's a label).
pub fn resolve_x64_register(name: &str) -> Option<X64Reg> {
    let lower = name.to_ascii_lowercase();
 
    if lower == "rip" {
        return Some(X64Reg::Rip);
    }
 
    if let Some((_, reg, slice)) = GPR_NAMES.iter().find(|(n, _, _)| *n == lower) {
        return Some(X64Reg::Gpr(*reg, *slice));
    }
 
    for (prefix, ctor) in [
        ("xmm", X64Reg::Xmm as fn(u8) -> X64Reg),
        ("ymm", X64Reg::Ymm as fn(u8) -> X64Reg),
        ("zmm", X64Reg::Zmm as fn(u8) -> X64Reg),
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if let Ok(n) = rest.parse::<u8>() {
                return Some(ctor(n));
            }
        }
    }
 
    None
}



/// One of the 16 general-purpose registers, identified by its 64-bit
/// name regardless of what width is currently being accessed. This is
/// the "physical register" identity — `Rax` names the same register
/// whether you're reading it as `rax`, `eax`, `ax`, `al`, or `ah`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64GpReg {
   Rax, Rbx, Rcx, Rdx, Rsi, Rdi, Rbp, Rsp,
   R8, R9, R10, R11, R12, R13, R14, R15,
}

/// Which bits of a `GpReg` an operand accesses.
///
/// This is what distinguishes `eax` from `rax` from `al` — same
/// register, different slice. It's also where the width-dependent
/// semantics live: writing `Low32` zero-extends the parent register
/// (x64's documented behavior), while writing `Low16`/`Low8`/`High8`
/// leaves the upper bits untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64GpSlice {
   Full,   // 64-bit: rax
   Low32,  // 32-bit: eax   — write zero-extends the full register
   Low16,  // 16-bit: ax    — write preserves upper bits
   Low8,   // low byte: al / r8b (r8b..r15b always available; sil/dil/bpl/spl need a REX prefix)
   High8,  // high byte: ah/bh/ch/dh only — legacy encoding, mutually exclusive with any REX prefix
}

impl X64GpSlice {
   pub fn width(self) -> Width {
       match self {
           X64GpSlice::Full => Width::W64,
           X64GpSlice::Low32 => Width::W32,
           X64GpSlice::Low16 => Width::W16,
           X64GpSlice::Low8 | X64GpSlice::High8 => Width::W8,
       }
   }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64Reg {
    Gpr(X64GpReg, X64GpSlice),
    Rip,
    Xmm(u8),
    Ymm(u8),
    Zmm(u8),
}
 
impl X64Reg {
    /// Bit width of this specific register access.
    pub fn width(self) -> Width {
        match self {
            X64Reg::Gpr(_, slice) => slice.width(),
            X64Reg::Rip => Width::W64,
            X64Reg::Xmm(_) => Width::W128,
            X64Reg::Ymm(_) => Width::W256,
            X64Reg::Zmm(_) => Width::W512,
        }
    }
 
    /// The underlying physical GPR, if this is a GPR access. Two
    /// `X64Reg`s with the same `parent_gpr()` alias the same storage —
    /// e.g. `X64Reg::Gpr(GpReg::Rax, GpSlice::Low32).parent_gpr()` and
    /// `X64Reg::Gpr(GpReg::Rax, GpSlice::Full).parent_gpr()` are both
    /// `Some(GpReg::Rax)`, telling you a write to one can affect the other.
    pub fn parent_gpr(self) -> Option<X64GpReg> {
        match self {
            X64Reg::Gpr(r, _) => Some(r),
            _ => None,
        }
    }
}
// ============================================================
// ARM64 operand kinds
// ============================================================
 
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arm64Reg {
    X(u8),  // 64-bit GPR view, x0-x30
    W(u8),  // 32-bit view of the same register
    V(u8),  // SIMD/FP register
    Sp,
    Xzr,    // zero register (reads 0, discards writes)
}

pub fn resolve_segment_register(name: &str) -> Option<SegmentReg> {
    match name.to_ascii_lowercase().as_str() {
        "cs" => Some(SegmentReg::Cs),
        "ds" => Some(SegmentReg::Ds),
        "es" => Some(SegmentReg::Es),
        "ss" => Some(SegmentReg::Ss),
        "fs" => Some(SegmentReg::Fs),
        "gs" => Some(SegmentReg::Gs),
        _ => None,
    }
}