use crate::translator::{
    instruction::Instruction,
    opcodes::Arm64Opcode,
    operand::{OperandKind, Role, X64OperandKind},
    register::Arm64Reg,
    translator::{TranslateError, Translator},
    util::{
        Width, arm64_instr, arm64_label_operand, imm_operand, map_mem_operand, map_mem_with_prep,
        map_register_operand, mem_operand, reg_operand, take2,
    },
};

impl Translator {
    pub fn translate_mov(&self, instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let [dest, src] = take2(&instr.operands);

        match (&dest.kind, &src.kind) {
            // reg <- reg
            (
                OperandKind::X64(X64OperandKind::Register(d)),
                OperandKind::X64(X64OperandKind::Register(s)),
            ) => {
                let (dr, dw) = map_register_operand(*d)?;
                let (sr, _) = map_register_operand(*s)?;
                Ok(vec![arm64_instr(
                    Arm64Opcode::Mov,
                    vec![
                        reg_operand(dr, dw, Role::Dest),
                        reg_operand(sr, dw, Role::Src),
                    ],
                )])
            }
            // reg <- [mem]
            (
                OperandKind::X64(X64OperandKind::Register(d)),
                OperandKind::X64(X64OperandKind::Memory(m)),
            ) => {
                let (dr, dw) = map_register_operand(*d)?;
                let scratch = self.alloc_scratch();
                let (mut instrs, am) = map_mem_with_prep(m, scratch)?;
                instrs.push(arm64_instr(
                    Arm64Opcode::Ldr,
                    vec![
                        reg_operand(dr, dw, Role::Dest),
                        mem_operand(am, dw, Role::Src),
                    ],
                ));
                Ok(instrs)
            }
            // [mem] <- reg
            (
                OperandKind::X64(X64OperandKind::Memory(m)),
                OperandKind::X64(X64OperandKind::Register(s)),
            ) => {
                let (sr, sw) = map_register_operand(*s)?;
                let scratch = self.alloc_scratch();
                let (mut instrs, am) = map_mem_with_prep(m, scratch)?;
                instrs.push(arm64_instr(
                    Arm64Opcode::Str,
                    vec![
                        mem_operand(am, sw, Role::Dest),
                        reg_operand(sr, sw, Role::Src),
                    ],
                ));
                Ok(instrs)
            }
            // reg <- imm
            (
                OperandKind::X64(X64OperandKind::Register(d)),
                OperandKind::X64(X64OperandKind::Immediate(n)),
            ) => {
                let (dr, dw) = map_register_operand(*d)?;
                // NOTE: real ARM64 immediate loads need movz/movk for values
                // that don't fit one 16-bit chunk — not handled here.
                Ok(vec![arm64_instr(
                    Arm64Opcode::Mov,
                    vec![
                        reg_operand(dr, dw, Role::Dest),
                        imm_operand(*n, dw, Role::Src),
                    ],
                )])
            }
            // [mem] <- imm: ARM64 has no store-immediate — materialise the
            // immediate in a scratch register, then store that register.
            (
                OperandKind::X64(X64OperandKind::Memory(m)),
                OperandKind::X64(X64OperandKind::Immediate(n)),
            ) => {
                let scratch = self.alloc_scratch();
                let am = map_mem_operand(m)?;
                let width = dest.width;
                Ok(vec![
                    arm64_instr(
                        Arm64Opcode::Mov,
                        vec![
                            reg_operand(scratch, width, Role::Dest),
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
            // reg <- label address (from OFFSET / OFFSET FLAT: operand)
            (
                OperandKind::X64(X64OperandKind::Register(d)),
                OperandKind::X64(X64OperandKind::Label(name)),
            ) => {
                let (dr, _) = map_register_operand(*d)?;
                // Force the X-register view — ADR always produces a 64-bit address.
                let x_dr = match dr {
                    Arm64Reg::W(n) => Arm64Reg::X(n),
                    other => other,
                };
                Ok(vec![arm64_instr(
                    Arm64Opcode::Adr,
                    vec![
                        reg_operand(x_dr, Width::W64, Role::Dest),
                        arm64_label_operand(name.clone(), Role::Src),
                    ],
                )])
            }
            // [mem] <- label: materialise the label address in a scratch register
            // then store it.  ARM64 has no "store immediate address" instruction.
            (
                OperandKind::X64(X64OperandKind::Memory(m)),
                OperandKind::X64(X64OperandKind::Label(name)),
            ) => {
                let scratch = self.alloc_scratch();
                let (mut instrs, am) = map_mem_with_prep(m, scratch)?;
                // Use a second scratch for the address; reuse the same slot since
                // alloc_scratch always returns the same register in the current
                // implementation and the prep (if any) won't alias it here.
                let addr_scratch = self.alloc_scratch();
                let x_scratch = match addr_scratch {
                    Arm64Reg::W(n) => Arm64Reg::X(n),
                    other => other,
                };
                instrs.push(arm64_instr(
                    Arm64Opcode::Adr,
                    vec![
                        reg_operand(x_scratch, Width::W64, Role::Dest),
                        arm64_label_operand(name.clone(), Role::Src),
                    ],
                ));
                instrs.push(arm64_instr(
                    Arm64Opcode::Str,
                    vec![
                        mem_operand(am, Width::W64, Role::Dest),
                        reg_operand(x_scratch, Width::W64, Role::Src),
                    ],
                ));
                Ok(instrs)
            }
            _ => Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "unsupported mov operand combination",
            }),
        }
    }
}
