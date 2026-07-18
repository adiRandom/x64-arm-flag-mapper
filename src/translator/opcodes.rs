
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64Condition {
    E, Ne,
    G, Ge, L, Le,       // signed
    A, Ae, B, Be,       // unsigned
    S, Ns, O, No, P, Np,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64Opcode {
    Mov, Add, Sub, Cmp, Test, Jmp, Call, Ret, Push, Pop, Mul, Lea,
    Xor, Inc, Dec, Jcc(X64Condition),
    // ...
}
 
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arm64Opcode {
    Mov, Add, Sub, Cmp, B, BCond, Bl, Ret, Ldr, Str, Ldp, Stp, Eor Tst
    // ...
}
 
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Opcode {
    X64(X64Opcode),
    Arm64(Arm64Opcode),
}