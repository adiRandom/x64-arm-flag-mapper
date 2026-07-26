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
    /// `cmp`/`test`: both operands are read-only on both ISAs.
    /// A memory operand on either side is loaded into a scratch register first.
    pub fn translate_cmp_test(
        &self,
        instr: &Instruction,
        arm_op: Arm64Opcode,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let [a, b] = take2(&instr.operands);

        // Resolve the first operand (may be a register or memory).
        let (a_reg, a_width, a_pre) = match &a.kind {
            OperandKind::X64(X64OperandKind::Register(ar)) => {
                let (ar, aw) = map_register_operand(*ar)?;
                (ar, aw, vec![])
            }
            OperandKind::X64(X64OperandKind::Memory(m)) => {
                let scratch = self.alloc_scratch();
                let am = map_mem_operand(m)?;
                let w = a.width;
                let load = arm64_instr(
                    Arm64Opcode::Ldr,
                    vec![
                        reg_operand(scratch, w, Role::Dest),
                        mem_operand(am, w, Role::Src),
                    ],
                );
                (scratch, w, vec![load])
            }
            _ => {
                return Err(TranslateError::Unsupported {
                    opcode: instr.opcode,
                    reason: "unsupported first operand for cmp/test",
                });
            }
        };

        // Resolve the second operand.
        let (b_operand, b_pre) = match &b.kind {
            OperandKind::X64(X64OperandKind::Register(br)) => {
                let (br, _) = map_register_operand(*br)?;
                (reg_operand(br, a_width, Role::Src), vec![])
            }
            OperandKind::X64(X64OperandKind::Immediate(n)) => {
                (imm_operand(*n, a_width, Role::Src), vec![])
            }
            OperandKind::X64(X64OperandKind::Memory(m)) => {
                let scratch = self.alloc_scratch();
                let am = map_mem_operand(m)?;
                let load = arm64_instr(
                    Arm64Opcode::Ldr,
                    vec![
                        reg_operand(scratch, a_width, Role::Dest),
                        mem_operand(am, a_width, Role::Src),
                    ],
                );
                (reg_operand(scratch, a_width, Role::Src), vec![load])
            }
            _ => {
                return Err(TranslateError::Unsupported {
                    opcode: instr.opcode,
                    reason: "unsupported second operand for cmp/test",
                });
            }
        };

        let cmp_instr = arm64_instr(
            arm_op,
            vec![reg_operand(a_reg, a_width, Role::Src), b_operand],
        );

        let mut result = a_pre;
        result.extend(b_pre);
        result.push(cmp_instr);
        Ok(result)
    }
}
