pub use crate::translator::operand::ArmConditionCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64Condition {
    E,
    Ne,
    G,
    Ge,
    L,
    Le, // signed
    A,
    Ae,
    B,
    Be, // unsigned
    S,
    Ns,
    O,
    No,
    P,
    Np,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64Opcode {
    Mov,
    Add,
    Sub,
    Cmp,
    Test,
    Jmp,
    Call,
    Ret,
    Push,
    Pop,
    Mul,
    Lea,
    Xor,
    Inc,
    Dec,
    Jcc(X64Condition),
    Nop,
    Leave,
    Cmov(X64Condition),
    // ...
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arm64Opcode {
    Mov,
    Add,
    Sub,
    Cmp,
    B,
    BCond(ArmConditionCode),
    Bl,
    Br,
    Blr,
    Ret,
    Ldr,
    Str,
    Ldp,
    Stp,
    Eor,
    Tst,
    And,  // bitwise AND; S-suffix (ands) when produces_flags = true
    Ldrb, // load byte, zero-extending
    Strb, // store byte
    Nop,
    Csel,
    Adr,
    // ...
}

/// Maps an x64 condition code to its ARM64 `ArmConditionCode` equivalent.
/// Returns `None` for P/Np (parity), which need software emulation.
pub fn map_condition(cond: X64Condition) -> Option<ArmConditionCode> {
    use X64Condition::*;
    match cond {
        E => Some(ArmConditionCode::Eq),
        Ne => Some(ArmConditionCode::Ne),
        G => Some(ArmConditionCode::Gt),
        Ge => Some(ArmConditionCode::Ge),
        L => Some(ArmConditionCode::Lt),
        Le => Some(ArmConditionCode::Le),
        A => Some(ArmConditionCode::Hi),
        Ae => Some(ArmConditionCode::Cs),
        B => Some(ArmConditionCode::Cc),
        Be => Some(ArmConditionCode::Ls),
        S => Some(ArmConditionCode::Mi),
        Ns => Some(ArmConditionCode::Pl),
        O => Some(ArmConditionCode::Vs),
        No => Some(ArmConditionCode::Vc),
        P | Np => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Opcode {
    X64(X64Opcode),
    Arm64(Arm64Opcode),
}
