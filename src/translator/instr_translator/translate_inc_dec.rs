use crate::translator::{
    instruction::Instruction,
    opcodes::Arm64Opcode,
    operand::{OperandKind, Role, X64OperandKind},
    translator::{TranslateError, Translator},
    util::{
        arm64_instr, imm_operand, map_mem_operand, map_register_operand, mem_operand, reg_operand,
        take1,
    },
};

impl Translator {
    /// x64 `inc`/`dec` have no immediate operand at all; ARM64 has no dedicated
    /// increment instruction either, so both become `add`/`sub dst, dst, #1`.
    ///
    /// When the operand is a memory location, the sequence is:
    /// `ldr scratch, [mem]; add/sub scratch, scratch, #1; str scratch, [mem]`.
    ///
    /// Known gap: x64 `inc`/`dec` leave the carry flag untouched (unlike
    /// `add`/`sub`) — since flags aren't modeled yet, that divergence is
    /// invisible here.
    pub fn translate_inc_dec(
        &self,
        instr: &Instruction,
        arm_op: Arm64Opcode,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let [dst] = take1(&instr.operands);

        match &dst.kind {
            OperandKind::X64(X64OperandKind::Register(d)) => {
                let (dr, dw) = map_register_operand(*d)?;
                Ok(vec![arm64_instr(
                    arm_op,
                    vec![
                        reg_operand(dr, dw, Role::Dest),
                        reg_operand(dr, dw, Role::Src),
                        imm_operand(1, dw, Role::Src),
                    ],
                )])
            }
            OperandKind::X64(X64OperandKind::Memory(m)) => {
                let scratch = self.alloc_scratch();
                let am = map_mem_operand(m)?;
                let width = dst.width;
                Ok(vec![
                    arm64_instr(
                        Arm64Opcode::Ldr,
                        vec![
                            reg_operand(scratch, width, Role::Dest),
                            mem_operand(am, width, Role::Src),
                        ],
                    ),
                    arm64_instr(
                        arm_op,
                        vec![
                            reg_operand(scratch, width, Role::Dest),
                            reg_operand(scratch, width, Role::Src),
                            imm_operand(1, width, Role::Src),
                        ],
                    ),
                    arm64_instr(
                        Arm64Opcode::Str,
                        vec![
                            mem_operand(am, width, Role::Dest),
                            reg_operand(scratch, width, Role::Src),
                        ],
                    ),
                ])
            }
            _ => Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "unsupported operand for inc/dec",
            }),
        }
    }
}
