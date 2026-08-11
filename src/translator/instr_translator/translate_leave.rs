use crate::translator::{
    arm_modifiers::Arm64Modifier,
    instruction::Instruction,
    opcodes::Arm64Opcode,
    operand::{Arm64MemOperand, Role},
    register::Arm64Reg,
    translator::{TranslateError, Translator},
    util::{Width, arm64_instr, imm_operand, mem_operand, reg_operand},
};

impl Translator {
    /// `leave` → `add sp, x29, #0` + `ldr x29, [sp], #8`
    pub fn translate_leave(
        &self,
        _instr: &Instruction,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let restore_sp = arm64_instr(
            Arm64Opcode::Add,
            vec![
                reg_operand(Arm64Reg::Sp, Width::W64, Role::Dest),
                reg_operand(Arm64Reg::X(29), Width::W64, Role::Src),
                imm_operand(0, Width::W64, Role::Src),
            ],
        );
        let pop_fp = arm64_instr(
            Arm64Opcode::Ldr,
            vec![
                reg_operand(Arm64Reg::X(29), Width::W64, Role::Dest),
                mem_operand(
                    Arm64MemOperand {
                        base: Arm64Reg::Sp,
                        offset: Some(8),
                        index: None,
                        modifier: Arm64Modifier::None,
                        pre_indexed: false,
                        post_indexed: true,
                    },
                    Width::W64,
                    Role::Src,
                ),
            ],
        );
        Ok(vec![restore_sp, pop_fp])
    }
}
