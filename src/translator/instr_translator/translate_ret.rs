use crate::translator::{
    instruction::Instruction,
    opcodes::Arm64Opcode,
    translator::{TranslateError, Translator},
    util::arm64_instr,
};

impl Translator {
    /// `ret` -> `ret`. Semantically not equivalent in isolation: x64's
    /// `call`/`ret` pair uses the hardware stack; ARM64's `bl`/`ret` uses the
    /// link register (`x30`). A translated function that itself calls other
    /// functions needs an explicit `x30` save/restore in its prologue/epilogue.
    pub fn translate_ret(&self, _instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        Ok(vec![arm64_instr(Arm64Opcode::Ret, vec![])])
    }
}
