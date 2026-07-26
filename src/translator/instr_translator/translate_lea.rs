use crate::translator::{
    arm_modifiers::{Arm64Modifier, ShiftKind},
    instruction::Instruction,
    opcodes::Arm64Opcode,
    operand::{Arm64OperandKind, Operand, OperandKind, Role, X64AddrBase, X64OperandKind},
    translator::{TranslateError, Translator},
    util::{arm64_instr, imm_operand, map_gpr, map_register_operand, reg_operand, take2},
};

impl Translator {
    pub fn translate_lea(&self, instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let [dest, src] = take2(&instr.operands);

        let (
            OperandKind::X64(X64OperandKind::Register(d)),
            OperandKind::X64(X64OperandKind::Memory(m)),
        ) = (&dest.kind, &src.kind)
        else {
            return Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "lea expects a register dest and memory src",
            });
        };
        let (dr, dw) = map_register_operand(*d)?;

        if m.segment.is_some() {
            return Err(TranslateError::SegmentOverrideNeedsSpecialHandling);
        }
        let base = match m.base {
            None => return Err(TranslateError::AbsoluteAddressingUnsupported),
            Some(X64AddrBase::Rip) => {
                return Err(TranslateError::RipRelativeNeedsAddressComputation);
            }
            Some(X64AddrBase::Reg(gpr)) => map_gpr(gpr),
        };

        match (m.index, m.disp) {
            (None, 0) => {
                // `lea rax, [rbx]` — the address *is* rbx's value.
                Ok(vec![arm64_instr(
                    Arm64Opcode::Mov,
                    vec![
                        reg_operand(dr, dw, Role::Dest),
                        reg_operand(base, dw, Role::Src),
                    ],
                )])
            }
            (None, disp) => Ok(vec![arm64_instr(
                Arm64Opcode::Add,
                vec![
                    reg_operand(dr, dw, Role::Dest),
                    reg_operand(base, dw, Role::Src),
                    imm_operand(disp as i64, dw, Role::Src),
                ],
            )]),
            (Some(idx), 0) => {
                let idx_reg = map_gpr(idx);
                let modifier = if m.scale == 1 {
                    Arm64Modifier::None
                } else {
                    Arm64Modifier::Shift(ShiftKind::Lsl, m.scale.trailing_zeros() as u8)
                };
                let idx_operand = Operand {
                    kind: OperandKind::Arm64(Arm64OperandKind::Register(idx_reg, modifier)),
                    width: dw,
                    role: Role::Src,
                };
                Ok(vec![arm64_instr(
                    Arm64Opcode::Add,
                    vec![
                        reg_operand(dr, dw, Role::Dest),
                        reg_operand(base, dw, Role::Src),
                        idx_operand,
                    ],
                )])
            }
            (Some(_), _) => Err(TranslateError::CombinedIndexAndDisplacementUnsupported),
        }
    }
}
