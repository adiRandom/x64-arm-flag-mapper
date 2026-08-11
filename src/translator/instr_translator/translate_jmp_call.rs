use crate::translator::arm_modifiers::Arm64Modifier;
use crate::translator::cpu_info::{CPU_INFO_REG, offsets};
use crate::translator::instruction::Instruction;
use crate::translator::opcodes::{Arm64Opcode, ArmConditionCode, X64Condition, map_condition};
use crate::translator::operand::{Arm64MemOperand, OperandKind, Role, X64OperandKind};
use crate::translator::translator::{TranslateError, Translator};
use crate::translator::util::{
    Width, arm64_instr, arm64_label_operand, imm_operand, map_mem_operand, map_register_operand,
    mem_operand, reg_operand, take1,
};

impl Translator {
    /// `jmp` → `B label` (label target), `BR reg` (register target),
    /// or `LDR scratch, [mem]` + `BR scratch` (memory-indirect target).
    pub fn translate_jmp(&self, instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let [target] = take1(&instr.operands);
        match &target.kind {
            OperandKind::X64(X64OperandKind::Label(name)) => Ok(vec![arm64_instr(
                Arm64Opcode::B,
                vec![arm64_label_operand(name.clone(), Role::Src)],
            )]),
            OperandKind::X64(X64OperandKind::Register(reg)) => {
                let (arm_reg, _) = map_register_operand(*reg)?;
                Ok(vec![arm64_instr(
                    Arm64Opcode::Br,
                    vec![reg_operand(arm_reg, Width::W64, Role::Src)],
                )])
            }
            OperandKind::X64(X64OperandKind::Memory(m)) => {
                let scratch = self.alloc_scratch();
                let am = map_mem_operand(m)?;
                Ok(vec![
                    arm64_instr(
                        Arm64Opcode::Ldr,
                        vec![
                            reg_operand(scratch, Width::W64, Role::Dest),
                            mem_operand(am, Width::W64, Role::Src),
                        ],
                    ),
                    arm64_instr(
                        Arm64Opcode::Br,
                        vec![reg_operand(scratch, Width::W64, Role::Src)],
                    ),
                ])
            }
            _ => Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "unsupported jmp target kind",
            }),
        }
    }

    /// `jcc` → `B.cond label` for conditions with a direct ARM64 equivalent.
    ///
    /// `jp`/`jnp` (parity) have no ARM64 flag equivalent and are emulated via
    /// the cpu-info struct: the parity byte is loaded from `[x28, #PF_OFFSET]`,
    /// compared with zero, and the appropriate `B.NE` / `B.EQ` is emitted.
    pub fn translate_jcc(
        &self,
        instr: &Instruction,
        cond: X64Condition,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let [target] = take1(&instr.operands);

        // Handle parity conditions through the cpu-info struct.
        if matches!(cond, X64Condition::P | X64Condition::Np) {
            return self.translate_jcc_parity(instr, cond, target);
        }

        // All other conditions have a direct ARM64 equivalent.
        let arm_cond = map_condition(cond).expect("non-parity condition always maps");
        match &target.kind {
            OperandKind::X64(X64OperandKind::Label(name)) => Ok(vec![arm64_instr(
                Arm64Opcode::BCond(arm_cond),
                vec![arm64_label_operand(name.clone(), Role::Src)],
            )]),
            _ => Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "jcc with non-label target not supported",
            }),
        }
    }

    /// `jp`/`jnp` emulation via the cpu-info parity byte.
    ///
    /// ```text
    /// ldrb  w12, [x28, #PF_OFFSET]   ; load parity byte
    /// cmp   x12, #0                   ; test against zero
    /// b.ne  label                     ; jp  (PF=1 → even → non-zero byte)
    ///   — or —
    /// b.eq  label                     ; jnp (PF=0 → odd  → zero byte)
    /// ```
    fn translate_jcc_parity(
        &self,
        instr: &Instruction,
        cond: X64Condition,
        target: &crate::translator::operand::Operand,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let name = match &target.kind {
            OperandKind::X64(X64OperandKind::Label(n)) => n,
            _ => {
                return Err(TranslateError::Unsupported {
                    opcode: instr.opcode,
                    reason: "jp/jnp with non-label target not supported",
                });
            }
        };

        let s = self.alloc_scratch();
        let ws = match s {
            crate::translator::register::Arm64Reg::X(n) => {
                crate::translator::register::Arm64Reg::W(n)
            }
            other => other,
        };

        // Branch condition: jp branches when PF=1 (byte != 0); jnp when PF=0.
        let branch_cond = match cond {
            X64Condition::P => ArmConditionCode::Ne,
            X64Condition::Np => ArmConditionCode::Eq,
            _ => unreachable!(),
        };

        Ok(vec![
            // Load the parity byte (stored by the preceding CMP/TST emulation).
            arm64_instr(
                Arm64Opcode::Ldrb,
                vec![
                    reg_operand(ws, Width::W32, Role::Dest),
                    mem_operand(
                        Arm64MemOperand {
                            base: CPU_INFO_REG,
                            offset: Some(offsets::PARITY_FLAG),
                            index: None,
                            modifier: Arm64Modifier::None,
                            pre_indexed: false,
                            post_indexed: false,
                        },
                        Width::W8,
                        Role::Src,
                    ),
                ],
            ),
            arm64_instr(
                Arm64Opcode::Cmp,
                vec![
                    reg_operand(s, Width::W64, Role::Src),
                    imm_operand(0, Width::W64, Role::Src),
                ],
            ),
            arm64_instr(
                Arm64Opcode::BCond(branch_cond),
                vec![arm64_label_operand(name.clone(), Role::Src)],
            ),
        ])
    }

    /// `call` → `BL label` (label target), `BLR reg` (register target),
    /// or `LDR scratch, [mem]` + `BLR scratch` (memory-indirect target).
    pub fn translate_call(&self, instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let [target] = take1(&instr.operands);
        match &target.kind {
            OperandKind::X64(X64OperandKind::Label(name)) => Ok(vec![arm64_instr(
                Arm64Opcode::Bl,
                vec![arm64_label_operand(name.clone(), Role::Src)],
            )]),
            OperandKind::X64(X64OperandKind::Register(reg)) => {
                let (arm_reg, _) = map_register_operand(*reg)?;
                Ok(vec![arm64_instr(
                    Arm64Opcode::Blr,
                    vec![reg_operand(arm_reg, Width::W64, Role::Src)],
                )])
            }
            OperandKind::X64(X64OperandKind::Memory(m)) => {
                let scratch = self.alloc_scratch();
                let am = map_mem_operand(m)?;
                Ok(vec![
                    arm64_instr(
                        Arm64Opcode::Ldr,
                        vec![
                            reg_operand(scratch, Width::W64, Role::Dest),
                            mem_operand(am, Width::W64, Role::Src),
                        ],
                    ),
                    arm64_instr(
                        Arm64Opcode::Blr,
                        vec![reg_operand(scratch, Width::W64, Role::Src)],
                    ),
                ])
            }
            _ => Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "unsupported call target kind",
            }),
        }
    }
}
