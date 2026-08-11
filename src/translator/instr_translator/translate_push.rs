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
    /// Translates `push reg` to a 16-byte-aligned ARM64 store.
    ///
    /// * `push rbp` (x29) → `stp x29, x30, [sp, #-16]!`
    ///   Saves the frame pointer and the link register together — the standard
    ///   ARM64 non-leaf prologue pair.  x30 must be saved by any function that
    ///   calls another (because `bl` overwrites it), so bundling the two saves
    ///   here is both correct and efficient.
    ///
    /// * Any other register → `str Xreg, [sp, #-16]!`
    ///   Single-register store with a 16-byte SP adjustment keeps SP aligned.
    pub fn translate_push(&self, instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let [src] = take1(&instr.operands);

        let OperandKind::X64(X64OperandKind::Register(s)) = &src.kind else {
            return Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "only register operands supported for push",
            });
        };
        let (sr, sw) = map_register_operand(*s)?;

        let pre_16 = Arm64MemOperand {
            base: Arm64Reg::Sp,
            offset: Some(-16),
            index: None,
            modifier: Arm64Modifier::None,
            pre_indexed: true,
            post_indexed: false,
        };

        if sr == Arm64Reg::X(29) {
            // push rbp: save fp + lr together (stp x29, x30, [sp, #-16]!)
            Ok(vec![arm64_instr(
                Arm64Opcode::Stp,
                vec![
                    reg_operand(Arm64Reg::X(29), Width::W64, Role::Src),
                    reg_operand(Arm64Reg::X(30), Width::W64, Role::Src),
                    mem_operand(pre_16, Width::W64, Role::Dest),
                ],
            )])
        } else {
            // push other: single register, 16-byte-aligned adjustment
            Ok(vec![arm64_instr(
                Arm64Opcode::Str,
                vec![
                    mem_operand(pre_16, sw, Role::Dest),
                    reg_operand(sr, sw, Role::Src),
                ],
            )])
        }
    }
}
