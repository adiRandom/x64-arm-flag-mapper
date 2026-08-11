use crate::translator::{
    arm_modifiers::{Arm64Modifier, ShiftKind},
    flags::FlagSet,
    instruction::{Arch, Instruction},
    opcodes::{Arm64Opcode, Opcode},
    operand::{
        Arm64MemOperand, Arm64OperandKind, ArmConditionCode, Operand, OperandKind, Role,
        X64AddrBase, X64MemOperand,
    },
    register::{Arm64Reg, X64GpReg, X64GpSlice, X64Reg},
    translator::TranslateError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Width {
    W8,
    W16,
    W32,
    W64,
    W128,
    W256,
    W512,
}

// ============================================================
// Register / operand conversion helpers
// ============================================================

pub fn map_gpr(reg: X64GpReg) -> Arm64Reg {
    match reg {
        // Argument / return registers
        X64GpReg::Rdi => Arm64Reg::X(0),
        X64GpReg::Rsi => Arm64Reg::X(1),
        X64GpReg::Rdx => Arm64Reg::X(2),
        X64GpReg::Rcx => Arm64Reg::X(3),
        X64GpReg::R8 => Arm64Reg::X(4),
        X64GpReg::R9 => Arm64Reg::X(5),
        // Return value / caller-saved scratch
        X64GpReg::Rax => Arm64Reg::X(9),
        // Caller-saved temporaries
        X64GpReg::R10 => Arm64Reg::X(10),
        X64GpReg::R11 => Arm64Reg::X(11),
        // Callee-saved registers
        X64GpReg::Rbx => Arm64Reg::X(19),
        X64GpReg::R12 => Arm64Reg::X(20),
        X64GpReg::R13 => Arm64Reg::X(21),
        X64GpReg::R14 => Arm64Reg::X(22),
        X64GpReg::R15 => Arm64Reg::X(23),
        // Frame and stack pointers
        X64GpReg::Rbp => Arm64Reg::X(29),
        X64GpReg::Rsp => Arm64Reg::Sp,
    }
}

/// Maps a full x64 register *access* (not just the physical register) to
/// its ARM64 equivalent, respecting width: a `Low32` access becomes the
/// `W`-view of the same slot, matching the zero-extend symmetry discussed
/// earlier. 16/8-bit accesses have no ARM64 register-file equivalent.
pub fn map_gpr_operand(
    reg: X64GpReg,
    slice: X64GpSlice,
) -> Result<(Arm64Reg, Width), TranslateError> {
    let mapped = map_gpr(reg);
    match (mapped, slice) {
        (Arm64Reg::Sp, X64GpSlice::Full) => Ok((Arm64Reg::Sp, Width::W64)),
        (Arm64Reg::X(n), X64GpSlice::Full) => Ok((Arm64Reg::X(n), Width::W64)),
        (Arm64Reg::X(n), X64GpSlice::Low32) => Ok((Arm64Reg::W(n), Width::W32)),
        _ => Err(TranslateError::UnsupportedRegisterWidth { reg, slice }),
    }
}

pub fn map_register_operand(x64reg: X64Reg) -> Result<(Arm64Reg, Width), TranslateError> {
    match x64reg {
        X64Reg::Gpr(reg, slice) => map_gpr_operand(reg, slice),
        other => Err(TranslateError::UnsupportedRegisterKind { reg: other }),
    }
}

pub fn reg_operand(reg: Arm64Reg, width: Width, role: Role) -> Operand {
    // ARM64 uses W registers for 32-bit operations and X for 64-bit.
    // alloc_scratch() always returns X(n); coerce it to W(n) here so
    // callers don't have to remember to do the conversion themselves.
    let reg = match (reg, width) {
        (Arm64Reg::X(n), Width::W32) => Arm64Reg::W(n),
        _ => reg,
    };
    Operand {
        kind: OperandKind::Arm64(Arm64OperandKind::Register(reg, Arm64Modifier::None)),
        width,
        role,
    }
}

/// Like `reg_operand` but with a shift modifier, e.g. for
/// `eor x0, x0, x0, lsr #4` (used in parity computation).
pub fn shifted_reg_operand(
    reg: Arm64Reg,
    shift: ShiftKind,
    amount: u8,
    width: Width,
    role: Role,
) -> Operand {
    Operand {
        kind: OperandKind::Arm64(Arm64OperandKind::Register(
            reg,
            Arm64Modifier::Shift(shift, amount),
        )),
        width,
        role,
    }
}

pub fn condition_operand(cc: ArmConditionCode, role: Role) -> Operand {
    Operand {
        kind: OperandKind::Arm64(Arm64OperandKind::Condition(cc)),
        width: Width::W64,
        role,
    }
}

pub fn arm64_label_operand(name: String, role: Role) -> Operand {
    Operand {
        kind: OperandKind::Arm64(Arm64OperandKind::Label(name)),
        width: Width::W64,
        role,
    }
}

pub fn imm_operand(value: i64, width: Width, role: Role) -> Operand {
    Operand {
        kind: OperandKind::Arm64(Arm64OperandKind::Immediate(value)),
        width,
        role,
    }
}

pub fn mem_operand(mem: Arm64MemOperand, width: Width, role: Role) -> Operand {
    Operand {
        kind: OperandKind::Arm64(Arm64OperandKind::Memory(mem)),
        width,
        role,
    }
}

pub fn arm64_instr(opcode: Arm64Opcode, operands: Vec<Operand>) -> Instruction {
    Instruction {
        arch: Arch::Arm64,
        opcode: Opcode::Arm64(opcode),
        operands,
        address: 0,
        length: 4,
        produces_flags: false,
        flags_written: FlagSet::NONE,
        flags_read: FlagSet::NONE,
    }
}

/// Converts an x64 memory operand's addressing into an ARM64 one.
/// Rejects (rather than mistranslates) the addressing shapes that have
/// no single-instruction ARM64 equivalent — see each `TranslateError`
/// variant's doc comment for why.
pub fn map_mem_operand(mem: &X64MemOperand) -> Result<Arm64MemOperand, TranslateError> {
    if mem.segment.is_some() {
        return Err(TranslateError::SegmentOverrideNeedsSpecialHandling);
    }
    let base = match mem.base {
        None => return Err(TranslateError::AbsoluteAddressingUnsupported),
        Some(X64AddrBase::Rip) => return Err(TranslateError::RipRelativeNeedsAddressComputation),
        Some(X64AddrBase::Reg(gpr)) => map_gpr(gpr),
    };

    match (mem.index, mem.disp) {
        (Some(idx), 0) => {
            let modifier = if mem.scale == 1 {
                Arm64Modifier::None
            } else {
                Arm64Modifier::Shift(ShiftKind::Lsl, mem.scale.trailing_zeros() as u8)
            };
            Ok(Arm64MemOperand {
                base,
                offset: None,
                index: Some(map_gpr(idx)),
                modifier,
                pre_indexed: false,
                post_indexed: false,
            })
        }
        (None, disp) => Ok(Arm64MemOperand {
            base,
            offset: Some(disp),
            index: None,
            modifier: Arm64Modifier::None,
            pre_indexed: false,
            post_indexed: false,
        }),
        (Some(_), _) => Err(TranslateError::CombinedIndexAndDisplacementUnsupported),
    }
}

pub fn take1(ops: &[Operand]) -> [&Operand; 1] {
    [&ops[0]]
}

pub fn take2(ops: &[Operand]) -> [&Operand; 2] {
    [&ops[0], &ops[1]]
}
