use crate::translator::{
    instruction::Instruction,
    opcodes::{Arm64Opcode, X64Condition, map_condition},
    operand::{OperandKind, Role, X64OperandKind},
    register::{Arm64Reg, X64Reg},
    translator::{TranslateError, Translator},
    util::{Width, arm64_instr, condition_operand, map_gpr, reg_operand, take1},
};

impl Translator {
    /// `setCC r/m8` → `cset Wd, cond`
    ///
    /// x86 `setCC` writes 0 or 1 to an 8-bit register based on the condition
    /// flags.  ARM64's `CSET Wd, cond` writes 0 or 1 to the 32-bit register
    /// view (automatically zero-extending to 64 bits).  Because `setCC` is
    /// almost always followed by `movzx reg, byte_reg` which corrects the
    /// upper bits, the small semantic difference (ARM64 zeroes bits 8-63 while
    /// x86 leaves them untouched) is irrelevant in practice.
    pub fn translate_setcc(
        &self,
        instr: &Instruction,
        cond: X64Condition,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let arm_cond = map_condition(cond).ok_or(TranslateError::Unsupported {
            opcode: instr.opcode,
            reason: "setp/setnp (parity) not yet emulated",
        })?;

        let [dest] = take1(&instr.operands);

        // setcc always writes a byte register.  map_register_operand rejects
        // Low8/Low16 slices, so we extract the base GPR and force the W-view.
        let arm_reg = match dest.kind {
            OperandKind::X64(X64OperandKind::Register(reg)) => match reg {
                X64Reg::Gpr(gpr, _) => match map_gpr(gpr) {
                    Arm64Reg::X(n) => Arm64Reg::W(n),
                    _ => {
                        return Err(TranslateError::UnsupportedRegisterKind { reg });
                    }
                },
                _ => {
                    return Err(TranslateError::UnsupportedRegisterKind { reg });
                }
            },
            _ => {
                return Err(TranslateError::Unsupported {
                    opcode: instr.opcode,
                    reason: "setcc only supports a register destination",
                });
            }
        };

        Ok(vec![arm64_instr(
            Arm64Opcode::Cset,
            vec![
                reg_operand(arm_reg, Width::W32, Role::Dest),
                condition_operand(arm_cond, Role::Src),
            ],
        )])
    }
}
