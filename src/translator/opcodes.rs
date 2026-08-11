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
          // ...
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Opcode {
    X64(X64Opcode),
    Arm64(Arm64Opcode),
}
