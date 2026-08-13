use crate::translator::{
    instruction::Instruction,
    opcodes::Arm64Opcode,
    operand::{OperandKind, Role, X64OperandKind},
    register::{Arm64Reg, X64GpReg, X64GpSlice, X64Reg},
    translator::{TranslateError, Translator},
    util::{Width, arm64_instr, map_gpr, map_register_operand, reg_operand, take2},
};

impl Translator {
    /// `movzx dest, src8/16` → `uxtb Wd, Wn` / `uxth Wd, Wn`
    ///
    /// Zero-extends an 8-bit or 16-bit source into a wider destination.
    /// ARM64's `UXTB`/`UXTH` always write to a W register (which automatically
    /// zero-extends to 64 bits), so the destination width from the x86 encoding
    /// is implicitly satisfied regardless of whether it was 32 or 64 bits.
    pub fn translate_movzx(&self, instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let [dest, src] = take2(&instr.operands);

        let (
            OperandKind::X64(X64OperandKind::Register(d)),
            OperandKind::X64(X64OperandKind::Register(s)),
        ) = (&dest.kind, &src.kind)
        else {
            return Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "movzx only supports register operands",
            });
        };

        // Destination: map normally (Low32 or Full).
        let (dr, dw) = map_register_operand(*d)?;

        // Source: extract the base GPR and use the W-register view.
        // map_register_operand rejects Low8/Low16, so we handle them here.
        let (sr, src_w) = match s {
            X64Reg::Gpr(gpr, slice) => {
                let arm = match map_gpr(*gpr) {
                    Arm64Reg::X(n) => Arm64Reg::W(n),
                    other => other,
                };
                let w = match slice {
                    X64GpSlice::Low8 => Width::W8,
                    X64GpSlice::Low16 => Width::W16,
                    _ => {
                        return Err(TranslateError::Unsupported {
                            opcode: instr.opcode,
                            reason: "movzx source must be an 8-bit or 16-bit register",
                        });
                    }
                };
                (arm, w)
            }
            _ => {
                return Err(TranslateError::Unsupported {
                    opcode: instr.opcode,
                    reason: "movzx source must be a GPR",
                });
            }
        };

        let arm_op = match src_w {
            Width::W8 => Arm64Opcode::Uxtb,
            Width::W16 => Arm64Opcode::Uxth,
            _ => unreachable!(),
        };

        Ok(vec![arm64_instr(
            arm_op,
            vec![
                reg_operand(dr, dw, Role::Dest),
                reg_operand(sr, Width::W32, Role::Src),
            ],
        )])
    }

    /// `movsxd dest64, src32` → `sxtw Xd, Wn`
    ///
    /// Sign-extends a 32-bit register into a 64-bit register.
    pub fn translate_movsxd(
        &self,
        instr: &Instruction,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let [dest, src] = take2(&instr.operands);

        let (
            OperandKind::X64(X64OperandKind::Register(d)),
            OperandKind::X64(X64OperandKind::Register(s)),
        ) = (&dest.kind, &src.kind)
        else {
            return Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "movsxd only supports register operands",
            });
        };

        // Destination must be a 64-bit register (X-view).
        let (dr, _) = map_register_operand(*d)?;
        let x_dest = match dr {
            Arm64Reg::X(n) => Arm64Reg::X(n),
            Arm64Reg::W(n) => Arm64Reg::X(n), // promote if needed
            _ => {
                return Err(TranslateError::Unsupported {
                    opcode: instr.opcode,
                    reason: "movsxd destination must be a 64-bit GPR",
                });
            }
        };

        // Source must be a 32-bit register (W-view).
        let (sr, _) = map_register_operand(*s)?;
        let w_src = match sr {
            Arm64Reg::W(n) => Arm64Reg::W(n),
            Arm64Reg::X(n) => Arm64Reg::W(n), // take W-view
            _ => {
                return Err(TranslateError::Unsupported {
                    opcode: instr.opcode,
                    reason: "movsxd source must be a 32-bit GPR",
                });
            }
        };

        Ok(vec![arm64_instr(
            Arm64Opcode::Sxtw,
            vec![
                reg_operand(x_dest, Width::W64, Role::Dest),
                reg_operand(w_src, Width::W32, Role::Src),
            ],
        )])
    }

    /// `cdqe` → `sxtw x9, w9`  (sign-extend EAX into RAX)
    ///
    /// `cdqe` has no explicit operands — it always operates on EAX/RAX.
    /// The ARM64 equivalent is `sxtw x9, w9` using our rax↔x9 mapping.
    pub fn translate_cdqe(&self, _instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let rax_x = map_gpr(X64GpReg::Rax); // X(9)
        let (x_reg, w_reg) = match rax_x {
            Arm64Reg::X(n) => (Arm64Reg::X(n), Arm64Reg::W(n)),
            _ => unreachable!("rax always maps to an X register"),
        };

        Ok(vec![arm64_instr(
            Arm64Opcode::Sxtw,
            vec![
                reg_operand(x_reg, Width::W64, Role::Dest),
                reg_operand(w_reg, Width::W32, Role::Src),
            ],
        )])
    }
}
