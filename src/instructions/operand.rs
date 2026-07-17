use crate::instructions::util::Width;

use super::register::*;
use super::arm_modifiers::*;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Src,
    Dest,
    SrcDest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Operand {
    pub kind: OperandKind,
    pub width: Width,
    pub role: Role,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct X64MemOperand {
    pub base: Option<X64Reg>,
    pub index: Option<X64Reg>,
    pub scale: u8, // 1, 2, 4, 8
    pub disp: i32,
    pub segment: Option<X64Reg>, // fs/gs override, if any
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Arm64MemOperand {
   pub base: Arm64Reg,
   pub offset: Option<i32>,
   pub index: Option<Arm64Reg>,
   pub modifier: Arm64Modifier,
   pub pre_indexed: bool,
   pub post_indexed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConditionCode {
    Eq, Ne, Cs, Cc, Mi, Pl, Vs, Vc, Hi, Ls, Ge, Lt, Gt, Le, Al,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arm64OperandKind {
    Register(Arm64Reg, Arm64Modifier),
    Immediate(i64),
    Memory(Arm64MemOperand),
    Condition(ConditionCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64OperandKind {
    Register(X64Reg),
    Immediate(i64),
    Memory(X64MemOperand),
    RelOffset(i32), // jmp/call/jcc targets
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperandKind {
    X64(X64OperandKind),
    Arm64(Arm64OperandKind),
}
 