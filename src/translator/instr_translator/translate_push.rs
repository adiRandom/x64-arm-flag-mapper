use crate::translator::{
    arm_modifiers::Arm64Modifier,
    instruction::Instruction,
    opcodes::Arm64Opcode,
    operand::{Arm64MemOperand, OperandKind, Role, X64OperandKind},
    register::Arm64Reg,
    translator::{TranslateError, Translator},
    util::{arm64_instr, map_register_operand, mem_operand, reg_operand, take1},
};

impl Translator {
    /// `push reg` -> `str reg, [sp, #-8]!` (pre-indexed: decrement sp *first*,
    /// then store). Register operand only for now.
    ///
    /// Known gap: this doesn't enforce ARM64's 16-byte stack-alignment
    /// convention — a single 8-byte push leaves `sp` misaligned relative to
    /// what AAPCS64 expects at a call boundary.
    pub fn translate_push(&self, instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let [src] = take1(&instr.operands);

        let OperandKind::X64(X64OperandKind::Register(s)) = &src.kind else {
            return Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "only register operands supported for push",
            });
        };
        let (sr, sw) = map_register_operand(*s)?;

        let mem = Arm64MemOperand {
            base: Arm64Reg::Sp,
            offset: Some(-8),
            index: None,
            modifier: Arm64Modifier::None,
            pre_indexed: true,
            post_indexed: false,
        };
        Ok(vec![arm64_instr(
            Arm64Opcode::Str,
            vec![
                mem_operand(mem, sw, Role::Dest),
                reg_operand(sr, sw, Role::Src),
            ],
        )])
    }
}
