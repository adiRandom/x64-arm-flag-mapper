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
    /// `pop reg` -> `ldr reg, [sp], #8` (post-indexed: load *first*, then
    /// increment sp). Same stack-alignment caveat as `translate_push`.
    pub fn translate_pop(&self, instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let [dst] = take1(&instr.operands);

        let OperandKind::X64(X64OperandKind::Register(d)) = &dst.kind else {
            return Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "only register operands supported for pop",
            });
        };
        let (dr, dw) = map_register_operand(*d)?;

        let mem = Arm64MemOperand {
            base: Arm64Reg::Sp,
            offset: Some(8),
            index: None,
            modifier: Arm64Modifier::None,
            pre_indexed: false,
            post_indexed: true,
        };
        Ok(vec![arm64_instr(
            Arm64Opcode::Ldr,
            vec![
                reg_operand(dr, dw, Role::Dest),
                mem_operand(mem, dw, Role::Src),
            ],
        )])
    }
}
