use crate::translator::{
    instruction::Instruction,
    opcodes::Arm64Opcode,
    operand::{OperandKind, Role, X64OperandKind},
    translator::{TranslateError, Translator},
    util::{
        arm64_instr, imm_operand, map_mem_operand, map_register_operand, mem_operand, reg_operand,
        take2,
    },
};

impl Translator {
    /// `add`/`sub`/`xor`: x64's destructive 2-operand form (`op dst, src` means
    /// `dst = dst op src`) becomes ARM64's non-destructive 3-operand form.
    ///
    /// When either operand is a memory location, a scratch register is used to
    /// load/store through. For a memory destination the sequence is:
    /// `ldr scratch, [mem]; op scratch, scratch, src; str scratch, [mem]`.
    /// For a memory source: `ldr scratch, [mem]; op dst, dst, scratch`.
    pub fn translate_add_sub(
        &self,
        instr: &Instruction,
        arm_op: Arm64Opcode,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let [dst, src] = take2(&instr.operands);

        match (&dst.kind, &src.kind) {
            // reg op= reg
            (
                OperandKind::X64(X64OperandKind::Register(d)),
                OperandKind::X64(X64OperandKind::Register(s)),
            ) => {
                let (dr, dw) = map_register_operand(*d)?;
                let (sr, _) = map_register_operand(*s)?;
                Ok(vec![arm64_instr(
                    arm_op,
                    vec![
                        reg_operand(dr, dw, Role::Dest),
                        reg_operand(dr, dw, Role::Src),
                        reg_operand(sr, dw, Role::Src),
                    ],
                )])
            }
            // reg op= imm
            (
                OperandKind::X64(X64OperandKind::Register(d)),
                OperandKind::X64(X64OperandKind::Immediate(n)),
            ) => {
                let (dr, dw) = map_register_operand(*d)?;
                Ok(vec![arm64_instr(
                    arm_op,
                    vec![
                        reg_operand(dr, dw, Role::Dest),
                        reg_operand(dr, dw, Role::Src),
                        imm_operand(*n, dw, Role::Src),
                    ],
                )])
            }
            // reg op= [mem]: load memory into scratch, then operate.
            (
                OperandKind::X64(X64OperandKind::Register(d)),
                OperandKind::X64(X64OperandKind::Memory(m)),
            ) => {
                let (dr, dw) = map_register_operand(*d)?;
                let scratch = self.alloc_scratch();
                let am = map_mem_operand(m)?;
                Ok(vec![
                    arm64_instr(
                        Arm64Opcode::Ldr,
                        vec![
                            reg_operand(scratch, dw, Role::Dest),
                            mem_operand(am, dw, Role::Src),
                        ],
                    ),
                    arm64_instr(
                        arm_op,
                        vec![
                            reg_operand(dr, dw, Role::Dest),
                            reg_operand(dr, dw, Role::Src),
                            reg_operand(scratch, dw, Role::Src),
                        ],
                    ),
                ])
            }
            // [mem] op= reg: load, operate, store.
            (
                OperandKind::X64(X64OperandKind::Memory(m)),
                OperandKind::X64(X64OperandKind::Register(s)),
            ) => {
                let (sr, sw) = map_register_operand(*s)?;
                let scratch = self.alloc_scratch();
                let am = map_mem_operand(m)?;
                Ok(vec![
                    arm64_instr(
                        Arm64Opcode::Ldr,
                        vec![
                            reg_operand(scratch, sw, Role::Dest),
                            mem_operand(am, sw, Role::Src),
                        ],
                    ),
                    arm64_instr(
                        arm_op,
                        vec![
                            reg_operand(scratch, sw, Role::Dest),
                            reg_operand(scratch, sw, Role::Src),
                            reg_operand(sr, sw, Role::Src),
                        ],
                    ),
                    arm64_instr(
                        Arm64Opcode::Str,
                        vec![
                            mem_operand(am, sw, Role::Dest),
                            reg_operand(scratch, sw, Role::Src),
                        ],
                    ),
                ])
            }
            // [mem] op= imm: load, operate, store.
            (
                OperandKind::X64(X64OperandKind::Memory(m)),
                OperandKind::X64(X64OperandKind::Immediate(n)),
            ) => {
                let width = dst.width;
                let scratch = self.alloc_scratch();
                let am = map_mem_operand(m)?;
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
                            imm_operand(*n, width, Role::Src),
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
                reason: "unsupported source operand",
            }),
        }
    }
}
