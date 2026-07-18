use super::operand::*;
use super::opcodes::*;


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
    // Consider adding, once you need them:
    // pub flags_read: FlagSet,
    // pub flags_written: FlagSet,
    // pub raw_bytes: SmallVec<[u8; 15]>, // useful for a passthrough fallback path
}