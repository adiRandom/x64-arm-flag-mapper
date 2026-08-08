use super::flags::FlagSet;
use super::opcodes::*;
use super::operand::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arch {
    X64,
    Arm64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub arch: Arch,
    pub opcode: Opcode,
    pub operands: Vec<Operand>,
    /// Address of this instruction in the original binary.
    pub address: u64,
    /// Encoded length in bytes (fixed at 4 for AArch64; variable for x64).
    pub length: u8,
    /// When `true`, the emitter should use the flag-setting variant of this
    /// instruction (e.g. `adds` instead of `add`) so that subsequent
    /// conditional branches can read ARM64's NZCV flags.
    /// Always `false` by default; toggled to `true` by `flag_production_pass`.
    pub produces_flags: bool,
    /// Flags this instruction writes, regardless of their previous value.
    /// Always `FlagSet::NONE` for ARM64 instructions (ARM64 flag state is
    /// tracked separately via NZCV when needed).
    pub flags_written: FlagSet,
    /// Flags this instruction reads — i.e. whose current value affects
    /// the instruction's behaviour (conditional branches, `adc`, etc.).
    pub flags_read: FlagSet,
}
