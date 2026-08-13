use crate::translator::{
    instruction::Instruction,
    opcodes::Arm64Opcode,
    operand::Role,
    register::Arm64Reg,
    translator::{TranslateError, Translator},
    util::{Width, arm64_instr, reg_operand},
};

impl Translator {
    /// `ret` -> return-value fixup + cpu-info epilogue + `ret`.
    ///
    /// x86-64 returns values in `rax`, which the translator maps to `x9`.
    /// ARM64's calling convention expects the return value in `x0`, so a
    /// `mov x0, x9` is emitted first.  This is the symmetric counterpart of
    /// the `mov x9, x0` fixup inserted after every `bl`/`blr`.
    pub fn translate_ret(&self, _instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
        let ret_fixup = arm64_instr(
            Arm64Opcode::Mov,
            vec![
                reg_operand(Arm64Reg::X(0), Width::W64, Role::Dest),
                reg_operand(Arm64Reg::X(9), Width::W64, Role::Src),
            ],
        );
        let mut instrs = vec![ret_fixup];
        instrs.extend(self.emit_cpu_info_epilogue());
        instrs.push(arm64_instr(Arm64Opcode::Ret, vec![]));
        Ok(instrs)
    }
}
