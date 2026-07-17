use crate::instructions::util::Width;

const GPR_NAMES: &[(&str, GpReg, GpSlice)] = &[
    ("rax", GpReg::Rax, GpSlice::Full), ("eax", GpReg::Rax, GpSlice::Low32),
    ("ax", GpReg::Rax, GpSlice::Low16), ("al", GpReg::Rax, GpSlice::Low8), ("ah", GpReg::Rax, GpSlice::High8),
 
    ("rbx", GpReg::Rbx, GpSlice::Full), ("ebx", GpReg::Rbx, GpSlice::Low32),
    ("bx", GpReg::Rbx, GpSlice::Low16), ("bl", GpReg::Rbx, GpSlice::Low8), ("bh", GpReg::Rbx, GpSlice::High8),
 
    ("rcx", GpReg::Rcx, GpSlice::Full), ("ecx", GpReg::Rcx, GpSlice::Low32),
    ("cx", GpReg::Rcx, GpSlice::Low16), ("cl", GpReg::Rcx, GpSlice::Low8), ("ch", GpReg::Rcx, GpSlice::High8),
 
    ("rdx", GpReg::Rdx, GpSlice::Full), ("edx", GpReg::Rdx, GpSlice::Low32),
    ("dx", GpReg::Rdx, GpSlice::Low16), ("dl", GpReg::Rdx, GpSlice::Low8), ("dh", GpReg::Rdx, GpSlice::High8),
 
    ("rsi", GpReg::Rsi, GpSlice::Full), ("esi", GpReg::Rsi, GpSlice::Low32),
    ("si", GpReg::Rsi, GpSlice::Low16), ("sil", GpReg::Rsi, GpSlice::Low8),
 
    ("rdi", GpReg::Rdi, GpSlice::Full), ("edi", GpReg::Rdi, GpSlice::Low32),
    ("di", GpReg::Rdi, GpSlice::Low16), ("dil", GpReg::Rdi, GpSlice::Low8),
 
    ("rbp", GpReg::Rbp, GpSlice::Full), ("ebp", GpReg::Rbp, GpSlice::Low32),
    ("bp", GpReg::Rbp, GpSlice::Low16), ("bpl", GpReg::Rbp, GpSlice::Low8),
 
    ("rsp", GpReg::Rsp, GpSlice::Full), ("esp", GpReg::Rsp, GpSlice::Low32),
    ("sp", GpReg::Rsp, GpSlice::Low16), ("spl", GpReg::Rsp, GpSlice::Low8),
 
    ("r8", GpReg::R8, GpSlice::Full), ("r8d", GpReg::R8, GpSlice::Low32), ("r8w", GpReg::R8, GpSlice::Low16), ("r8b", GpReg::R8, GpSlice::Low8),
    ("r9", GpReg::R9, GpSlice::Full), ("r9d", GpReg::R9, GpSlice::Low32), ("r9w", GpReg::R9, GpSlice::Low16), ("r9b", GpReg::R9, GpSlice::Low8),
    ("r10", GpReg::R10, GpSlice::Full), ("r10d", GpReg::R10, GpSlice::Low32), ("r10w", GpReg::R10, GpSlice::Low16), ("r10b", GpReg::R10, GpSlice::Low8),
    ("r11", GpReg::R11, GpSlice::Full), ("r11d", GpReg::R11, GpSlice::Low32), ("r11w", GpReg::R11, GpSlice::Low16), ("r11b", GpReg::R11, GpSlice::Low8),
    ("r12", GpReg::R12, GpSlice::Full), ("r12d", GpReg::R12, GpSlice::Low32), ("r12w", GpReg::R12, GpSlice::Low16), ("r12b", GpReg::R12, GpSlice::Low8),
    ("r13", GpReg::R13, GpSlice::Full), ("r13d", GpReg::R13, GpSlice::Low32), ("r13w", GpReg::R13, GpSlice::Low16), ("r13b", GpReg::R13, GpSlice::Low8),
    ("r14", GpReg::R14, GpSlice::Full), ("r14d", GpReg::R14, GpSlice::Low32), ("r14w", GpReg::R14, GpSlice::Low16), ("r14b", GpReg::R14, GpSlice::Low8),
    ("r15", GpReg::R15, GpSlice::Full), ("r15d", GpReg::R15, GpSlice::Low32), ("r15w", GpReg::R15, GpSlice::Low16), ("r15b", GpReg::R15, GpSlice::Low8),
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
pub enum GpReg {
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
pub enum GpSlice {
   Full,   // 64-bit: rax
   Low32,  // 32-bit: eax   — write zero-extends the full register
   Low16,  // 16-bit: ax    — write preserves upper bits
   Low8,   // low byte: al / r8b (r8b..r15b always available; sil/dil/bpl/spl need a REX prefix)
   High8,  // high byte: ah/bh/ch/dh only — legacy encoding, mutually exclusive with any REX prefix
}

impl GpSlice {
   pub fn width(self) -> Width {
       match self {
           GpSlice::Full => Width::W64,
           GpSlice::Low32 => Width::W32,
           GpSlice::Low16 => Width::W16,
           GpSlice::Low8 | GpSlice::High8 => Width::W8,
       }
   }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64Reg {
    Gpr(GpReg, GpSlice),
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
    pub fn parent_gpr(self) -> Option<GpReg> {
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