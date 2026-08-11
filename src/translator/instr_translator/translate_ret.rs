use crate::translator::{
    instruction::Instruction,
    opcodes::Arm64Opcode,
    translator::{TranslateError, Translator},
    util::arm64_instr,
};

impl Translator {
    /// `ret` -> cpu-info epilogue + `ret`. Restores the caller's x28 (which
    /// the prologue saved) and deallocates the cpu-info struct before
    /// returning. Semantically not equivalent in isolation: x64's
    /// `call`/`ret` pair uses the hardware stack; ARM64's `bl`/`ret` uses the
    /// link register (`x30`). A translated function that itself calls other
    /// functions needs an explicit `x30` save/restore in its prologue/epilogue.
    pub fn translate_ret(&self, _instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let mut instrs = self.emit_cpu_info_epilogue();
        instrs.push(arm64_instr(Arm64Opcode::Ret, vec![]));
        Ok(instrs)
    }
}
