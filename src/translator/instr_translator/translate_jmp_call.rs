use crate::translator::{
    instruction::Instruction,
    opcodes::{Arm64Opcode, ArmConditionCode, X64Condition},
    operand::{OperandKind, Role, X64OperandKind},
    translator::{TranslateError, Translator},
    util::{
        Width, arm64_instr, arm64_label_operand, map_mem_operand, map_register_operand,
        mem_operand, reg_operand, take1,
    },
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

    /// `jcc` → `B.cond label`.
    ///
    /// The x64 condition is mapped to the ARM64 equivalent condition code.
    /// `jp`/`jnp` (parity) have no ARM64 equivalent and return `Unsupported`.
    pub fn translate_jcc(
        &self,
        instr: &Instruction,
        cond: X64Condition,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let arm_cond = map_condition(cond).ok_or(TranslateError::Unsupported {
            opcode: instr.opcode,
            reason: "parity flag (P/NP) has no ARM64 equivalent; software emulation required",
        })?;
        let [target] = take1(&instr.operands);
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

/// Maps an x64 condition code to its ARM64 `ConditionCode` equivalent.
/// Returns `None` for `P`/`Np` (parity), which ARM64 has no native equivalent for.
fn map_condition(cond: X64Condition) -> Option<ArmConditionCode> {
    use X64Condition::*;
    match cond {
        E => Some(ArmConditionCode::Eq),
        Ne => Some(ArmConditionCode::Ne),
        G => Some(ArmConditionCode::Gt),
        Ge => Some(ArmConditionCode::Ge),
        L => Some(ArmConditionCode::Lt),
        Le => Some(ArmConditionCode::Le),
        A => Some(ArmConditionCode::Hi),
        Ae => Some(ArmConditionCode::Cs), // carry set = unsigned >=
        B => Some(ArmConditionCode::Cc),  // carry clear = unsigned <
        Be => Some(ArmConditionCode::Ls),
        S => Some(ArmConditionCode::Mi),
        Ns => Some(ArmConditionCode::Pl),
        O => Some(ArmConditionCode::Vs),
        No => Some(ArmConditionCode::Vc),
        P | Np => None,
    }
}
