//! x86-64 flag tracking.
//!
//! `FlagSet` is a small bitset over the six arithmetic flags that matter for
//! translation: CF, PF, AF, ZF, SF, OF.  Every loaded x86-64 `Instruction`
//! carries a `flags_written` and a `flags_read` value so that later passes
//! can reason about flag liveness without re-deriving it from the opcode.

use crate::translator::opcodes::{X64Condition, X64Opcode};
use std::fmt;

/// A set of x86-64 arithmetic flags, stored as a bitmask.
///
/// The six flags covered are the ones written/read by normal arithmetic and
/// branch instructions.  DF (direction) and IF (interrupt) are not modelled
/// here — they are never set by user-space code in a way that a translator
/// needs to track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FlagSet(u8);

impl FlagSet {
    pub const NONE: Self = Self(0);
    pub const CF: Self = Self(1 << 0); // Carry
    pub const PF: Self = Self(1 << 1); // Parity
    pub const AF: Self = Self(1 << 2); // Adjust (auxiliary carry)
    pub const ZF: Self = Self(1 << 3); // Zero
    pub const SF: Self = Self(1 << 4); // Sign
    pub const OF: Self = Self(1 << 5); // Overflow

    /// All six arithmetic flags — the full set written by ADD, SUB, CMP, etc.
    const ARITH: Self = Self(0b0011_1111);
    /// The five flags written by INC/DEC — they intentionally leave CF alone
    /// so that code like `add rcx, 1` followed by `inc rdx` doesn't disturb
    /// a carry from the `add`.
    const ARITH_NO_CF: Self = Self(0b0011_1110);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns true if `self` contains *all* flags in `other`.
    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Returns true if `self` and `other` share at least one flag.
    pub fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}

impl std::ops::BitOr for FlagSet {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for FlagSet {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for FlagSet {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl fmt::Display for FlagSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            return write!(f, "∅");
        }
        let mut parts = Vec::new();
        if self.contains(Self::CF) {
            parts.push("CF");
        }
        if self.contains(Self::PF) {
            parts.push("PF");
        }
        if self.contains(Self::AF) {
            parts.push("AF");
        }
        if self.contains(Self::ZF) {
            parts.push("ZF");
        }
        if self.contains(Self::SF) {
            parts.push("SF");
        }
        if self.contains(Self::OF) {
            parts.push("OF");
        }
        write!(f, "{}", parts.join("|"))
    }
}

// ── per-opcode flag tables ────────────────────────────────────────────────────

/// Ordered list of each individual flag as a singleton `FlagSet`.
/// Index 0 = CF, 1 = PF, 2 = AF, 3 = ZF, 4 = SF, 5 = OF.
/// Used for iterating over flags one at a time.
pub const FLAG_BITS: [FlagSet; 6] = [
    FlagSet::CF,
    FlagSet::PF,
    FlagSet::AF,
    FlagSet::ZF,
    FlagSet::SF,
    FlagSet::OF,
];

/// Returns `(flags_written, flags_read)` for an x86-64 opcode.
///
/// "Written" means the instruction updates the flag regardless of its previous
/// value.  "Read" means the instruction's behaviour depends on the flag.
/// An instruction can do both (e.g. `adc` reads CF and then writes CF), but
/// the opcodes modelled here are simpler.
pub fn flags_for_opcode(opcode: X64Opcode) -> (FlagSet, FlagSet) {
    match opcode {
        // Full arithmetic set: CF PF AF ZF SF OF
        X64Opcode::Add | X64Opcode::Sub | X64Opcode::Xor | X64Opcode::Cmp | X64Opcode::Test => {
            (FlagSet::ARITH, FlagSet::NONE)
        }

        // INC/DEC write everything *except* CF — by design, so that a
        // preceding ADD's carry is not destroyed.
        X64Opcode::Inc | X64Opcode::Dec => (FlagSet::ARITH_NO_CF, FlagSet::NONE),

        // MUL defines CF and OF (result overflow indicators); the other four
        // flags are left architecturally undefined, so we don't model them as
        // written (a consumer can't rely on them).
        X64Opcode::Mul => (FlagSet::CF | FlagSet::OF, FlagSet::NONE),

        // Conditional branches read a specific subset of flags determined by
        // their condition code.
        X64Opcode::Jcc(cond) => (FlagSet::NONE, flags_read_by_condition(cond)),

        // Everything else neither reads nor writes the arithmetic flags.
        X64Opcode::Mov
        | X64Opcode::Lea
        | X64Opcode::Push
        | X64Opcode::Pop
        | X64Opcode::Jmp
        | X64Opcode::Call
        | X64Opcode::Ret => (FlagSet::NONE, FlagSet::NONE),
    }
}

/// The flags read by each Jcc condition code.
fn flags_read_by_condition(cond: X64Condition) -> FlagSet {
    use X64Condition::*;
    match cond {
        // ZF only
        E | Ne => FlagSet::ZF,
        // SF and OF (signed comparison: overflow can invert the sign)
        Ge | L => FlagSet::SF | FlagSet::OF,
        // ZF + SF + OF
        G | Le => FlagSet::ZF | FlagSet::SF | FlagSet::OF,
        // CF only (unsigned strict)
        Ae | B => FlagSet::CF,
        // CF + ZF (unsigned ≤ / >)
        A | Be => FlagSet::CF | FlagSet::ZF,
        // Single-flag conditions
        S | Ns => FlagSet::SF,
        O | No => FlagSet::OF,
        P | Np => FlagSet::PF,
    }
}
