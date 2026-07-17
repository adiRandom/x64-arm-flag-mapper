#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum X64Opcode {
    Mov, Add, Sub, Cmp, Test, Jmp, Jcc, Call, Ret, Push, Pop, Mul, Lea,
    // ...
}
 
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arm64Opcode {
    Mov, Add, Sub, Cmp, B, BCond, Bl, Ret, Ldr, Str, Ldp, Stp,
    // ...
}
 
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Opcode {
    X64(X64Opcode),
    Arm64(Arm64Opcode),
}