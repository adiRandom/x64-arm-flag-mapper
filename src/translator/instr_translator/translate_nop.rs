use crate::translator::{
    instruction::Instruction,
    opcodes::Arm64Opcode,
    translator::{TranslateError, Translator},
    util::arm64_instr,
};

impl Translator {
    pub fn translate_nop(&self, _instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        Ok(vec![arm64_instr(Arm64Opcode::Nop, vec![])])
    }
}
