use crate::translator::{
    arm_modifiers::Arm64Modifier,
    instruction::Instruction,
    opcodes::Arm64Opcode,
    operand::{Arm64MemOperand, OperandKind, Role, X64OperandKind},
    register::Arm64Reg,
    translator::{TranslateError, Translator},
    util::{Width, arm64_instr, map_register_operand, mem_operand, reg_operand, take1},
};

impl Translator {
    /// Translates `pop reg` to a 16-byte-aligned ARM64 load.
    ///
    /// * `pop rbp` (x29) → `ldp x29, x30, [sp], #16`
    ///   Restores frame pointer and link register together, matching the
    ///   `push rbp` → `stp x29, x30` prologue convention.
    ///
    /// * Any other register → `ldr Xreg, [sp], #16`
    ///   Post-indexed load; loads 8 bytes and advances SP by 16.
    pub fn translate_pop(&self, instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let [dst] = take1(&instr.operands);

        let OperandKind::X64(X64OperandKind::Register(d)) = &dst.kind else {
            return Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "only register operands supported for pop",
            });
        };
        let (dr, dw) = map_register_operand(*d)?;

        let post_16 = Arm64MemOperand {
            base: Arm64Reg::Sp,
            offset: Some(16),
            index: None,
            modifier: Arm64Modifier::None,
            pre_indexed: false,
            post_indexed: true,
        };

        if dr == Arm64Reg::X(29) {
            // pop rbp: restore fp + lr together (ldp x29, x30, [sp], #16)
            Ok(vec![arm64_instr(
                Arm64Opcode::Ldp,
                vec![
                    reg_operand(Arm64Reg::X(29), Width::W64, Role::Dest),
                    reg_operand(Arm64Reg::X(30), Width::W64, Role::Dest),
                    mem_operand(post_16, Width::W64, Role::Src),
                ],
            )])
        } else {
            // pop other: single register, 16-byte-aligned advance
            Ok(vec![arm64_instr(
                Arm64Opcode::Ldr,
                vec![
                    reg_operand(dr, dw, Role::Dest),
                    mem_operand(post_16, dw, Role::Src),
                ],
            )])
        }
    }
}
