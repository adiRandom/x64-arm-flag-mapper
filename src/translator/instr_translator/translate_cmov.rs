use crate::translator::{
    instruction::Instruction,
    opcodes::{Arm64Opcode, X64Condition, map_condition},
    operand::{OperandKind, Role, X64OperandKind},
    translator::{TranslateError, Translator},
    util::{arm64_instr, condition_operand, map_register_operand, reg_operand, take2},
};

impl Translator {
    /// `cmovCC dest, src` → `csel dest, src, dest, CC`
    pub fn translate_cmov(
        &self,
        instr: &Instruction,
        cond: X64Condition,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let arm_cond = map_condition(cond).ok_or(TranslateError::Unsupported {
            opcode: instr.opcode,
            reason: "cmovp/cmovnp (parity) not yet emulated",
        })?;

        let [dest, src] = take2(&instr.operands);

        match (&dest.kind, &src.kind) {
            (
                OperandKind::X64(X64OperandKind::Register(d)),
                OperandKind::X64(X64OperandKind::Register(s)),
            ) => {
                let (dr, dw) = map_register_operand(*d)?;
                let (sr, _) = map_register_operand(*s)?;
                Ok(vec![arm64_instr(
                    Arm64Opcode::Csel,
                    vec![
                        reg_operand(dr, dw, Role::Dest),
                        reg_operand(sr, dw, Role::Src), // true branch: src value
                        reg_operand(dr, dw, Role::Src), // false branch: keep dest
                        condition_operand(arm_cond, Role::Src),
                    ],
                )])
            }
            _ => Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "cmov with non-register operands not supported",
            }),
        }
    }
}
