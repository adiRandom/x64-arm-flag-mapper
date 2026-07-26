use crate::translator::{
    arm_modifiers::{Arm64Modifier, ShiftKind},
    instruction::{Arch, Instruction},
    opcodes::{Arm64Opcode, Opcode, X64Opcode},
    operand::{
        Arm64MemOperand, Arm64OperandKind, Operand, OperandKind, Role, X64AddrBase, X64MemOperand,
        X64OperandKind,
    },
    register::{Arm64Reg, X64GpReg, X64GpSlice, X64Reg},
    util::Width,
};

#[derive(Debug, Clone, PartialEq)]
pub enum TranslateError {
    AlreadyArm64,
    /// jmp/jcc/call need resolved branch targets, which requires the
    /// whole-program symbol-table pass discussed earlier — not built yet.
    NeedsLabelResolution {
        opcode: Opcode,
    },
    Unsupported {
        opcode: Opcode,
        reason: &'static str,
    },
    /// 16/8-bit sub-register operands have no ARM64 register-file
    /// equivalent (ARM64 only has W/X views) — would need masking or
    /// byte/half load-store instructions instead of a register swap.
    UnsupportedRegisterWidth {
        reg: X64GpReg,
        slice: X64GpSlice,
    },
    UnsupportedRegisterKind {
        reg: X64Reg,
    },
    /// `[rip + disp]` needs an `adrp`+`add` (and possibly `ldr`) sequence
    /// computed against real layout addresses — not a single operand swap.
    /// See the earlier rip-relative discussion.
    RipRelativeNeedsAddressComputation,
    /// `fs:`/`gs:` needs a `mrs x, tpidr_el0` first — see the segments discussion.
    SegmentOverrideNeedsSpecialHandling,
    /// x64's `[base + index*scale + disp]` with *both* an index and a
    /// nonzero displacement has no single ARM64 addressing-mode
    /// equivalent — ARM64 register-offset and immediate-offset forms are
    /// mutually exclusive. Needs an extra `add` to fold one in first.
    CombinedIndexAndDisplacementUnsupported,
    /// x64 memory operand with no base register at all (rare absolute
    /// addressing) — ARM64 addressing always needs a base register.
    AbsoluteAddressingUnsupported,
    /// ARM64 has no arithmetic-directly-on-memory or store-immediate
    /// instructions; both sides need to go through a register first.
    MemoryOperandNeedsScratchRegister,
}

impl std::fmt::Display for TranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranslateError::AlreadyArm64 => write!(f, "instruction is already ARM64"),
            TranslateError::NeedsLabelResolution { opcode } => {
                write!(
                    f,
                    "{opcode:?} needs resolved branch targets (label-resolution pass not implemented)"
                )
            }
            TranslateError::Unsupported { opcode, reason } => {
                write!(f, "{opcode:?} not supported: {reason}")
            }
            TranslateError::UnsupportedRegisterWidth { reg, slice } => {
                write!(f, "{reg:?}:{slice:?} has no ARM64 register equivalent")
            }
            TranslateError::UnsupportedRegisterKind { reg } => {
                write!(f, "{reg:?} not supported yet")
            }
            TranslateError::RipRelativeNeedsAddressComputation => {
                write!(
                    f,
                    "rip-relative operand needs an adrp/add address-computation sequence, not a single operand swap"
                )
            }
            TranslateError::SegmentOverrideNeedsSpecialHandling => {
                write!(
                    f,
                    "fs/gs-relative operand needs a tpidr_el0 read first, not a single operand swap"
                )
            }
            TranslateError::CombinedIndexAndDisplacementUnsupported => {
                write!(
                    f,
                    "base+index*scale+disp has no single ARM64 addressing mode; needs an extra 'add' to fold in the displacement"
                )
            }
            TranslateError::AbsoluteAddressingUnsupported => {
                write!(
                    f,
                    "memory operand with no base register isn't supported yet"
                )
            }
            TranslateError::MemoryOperandNeedsScratchRegister => {
                write!(
                    f,
                    "this operand needs to be loaded into a scratch register first; ARM64 has no memory-operand arithmetic or immediate stores"
                )
            }
        }
    }
}

impl Instruction {
    /// Translates one x64 instruction into zero or more ARM64
    /// instructions. Some x64 instructions need more than one ARM64
    /// instruction (`push`/`pop` don't, but a future `lea [rip+...]` will) —
    /// hence `Vec` rather than a 1:1 return.
    pub fn to_arm64(&self) -> Result<Vec<Instruction>, TranslateError> {
        let X64Opcode_ = match self.opcode {
            Opcode::X64(op) => op,
            Opcode::Arm64(_) => return Err(TranslateError::AlreadyArm64),
        };

        match X64Opcode_ {
            X64Opcode::Mov => translate_mov(self),
            X64Opcode::Lea => translate_lea(self),
            X64Opcode::Add => translate_add_sub(self, Arm64Opcode::Add),
            X64Opcode::Sub => translate_add_sub(self, Arm64Opcode::Sub),
            X64Opcode::Xor => translate_add_sub(self, Arm64Opcode::Eor),
            X64Opcode::Cmp => translate_cmp_test(self, Arm64Opcode::Cmp),
            X64Opcode::Test => translate_cmp_test(self, Arm64Opcode::Tst),
            X64Opcode::Inc => translate_inc_dec(self, Arm64Opcode::Add),
            X64Opcode::Dec => translate_inc_dec(self, Arm64Opcode::Sub),
            X64Opcode::Push => translate_push(self),
            X64Opcode::Pop => translate_pop(self),
            X64Opcode::Ret => translate_ret(self),
            X64Opcode::Jmp | X64Opcode::Jcc(_) | X64Opcode::Call => {
                Err(TranslateError::NeedsLabelResolution {
                    opcode: self.opcode,
                })
            }
            X64Opcode::Mul => Err(TranslateError::Unsupported {
                opcode: self.opcode,
                reason: "implicit rdx:rax destination isn't modeled as an operand yet",
            }),
        }
    }
}

// ============================================================
// Register / operand conversion helpers
// ============================================================

fn map_gpr(reg: X64GpReg) -> Arm64Reg {
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
fn map_gpr_operand(reg: X64GpReg, slice: X64GpSlice) -> Result<(Arm64Reg, Width), TranslateError> {
    let mapped = map_gpr(reg);
    match (mapped, slice) {
        (Arm64Reg::Sp, X64GpSlice::Full) => Ok((Arm64Reg::Sp, Width::W64)),
        (Arm64Reg::X(n), X64GpSlice::Full) => Ok((Arm64Reg::X(n), Width::W64)),
        (Arm64Reg::X(n), X64GpSlice::Low32) => Ok((Arm64Reg::W(n), Width::W32)),
        _ => Err(TranslateError::UnsupportedRegisterWidth { reg, slice }),
    }
}

fn map_register_operand(x64reg: X64Reg) -> Result<(Arm64Reg, Width), TranslateError> {
    match x64reg {
        X64Reg::Gpr(reg, slice) => map_gpr_operand(reg, slice),
        other => Err(TranslateError::UnsupportedRegisterKind { reg: other }),
    }
}

fn reg_operand(reg: Arm64Reg, width: Width, role: Role) -> Operand {
    Operand {
        kind: OperandKind::Arm64(Arm64OperandKind::Register(reg, Arm64Modifier::None)),
        width,
        role,
    }
}

fn imm_operand(value: i64, width: Width, role: Role) -> Operand {
    Operand {
        kind: OperandKind::Arm64(Arm64OperandKind::Immediate(value)),
        width,
        role,
    }
}

fn mem_operand(mem: Arm64MemOperand, width: Width, role: Role) -> Operand {
    Operand {
        kind: OperandKind::Arm64(Arm64OperandKind::Memory(mem)),
        width,
        role,
    }
}

fn arm64_instr(opcode: Arm64Opcode, operands: Vec<Operand>) -> Instruction {
    Instruction {
        arch: Arch::Arm64,
        opcode: Opcode::Arm64(opcode),
        operands,
        address: 0,
        length: 4,
    }
}

/// Converts an x64 memory operand's addressing into an ARM64 one.
/// Rejects (rather than mistranslates) the addressing shapes that have
/// no single-instruction ARM64 equivalent — see each `TranslateError`
/// variant's doc comment for why.
fn map_mem_operand(mem: &X64MemOperand) -> Result<Arm64MemOperand, TranslateError> {
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

// ============================================================
// Per-opcode translation
// ============================================================

fn translate_mov(instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
    let [dest, src] = take2(&instr.operands);

    match (&dest.kind, &src.kind) {
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
        (
            OperandKind::X64(X64OperandKind::Register(d)),
            OperandKind::X64(X64OperandKind::Memory(m)),
        ) => {
            let (dr, dw) = map_register_operand(*d)?;
            let am = map_mem_operand(m)?;
            Ok(vec![arm64_instr(
                Arm64Opcode::Ldr,
                vec![
                    reg_operand(dr, dw, Role::Dest),
                    mem_operand(am, dw, Role::Src),
                ],
            )])
        }
        (
            OperandKind::X64(X64OperandKind::Memory(m)),
            OperandKind::X64(X64OperandKind::Register(s)),
        ) => {
            let (sr, sw) = map_register_operand(*s)?;
            let am = map_mem_operand(m)?;
            Ok(vec![arm64_instr(
                Arm64Opcode::Str,
                vec![
                    mem_operand(am, sw, Role::Dest),
                    reg_operand(sr, sw, Role::Src),
                ],
            )])
        }
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
        (
            OperandKind::X64(X64OperandKind::Memory(_)),
            OperandKind::X64(X64OperandKind::Immediate(_)),
        ) => Err(TranslateError::MemoryOperandNeedsScratchRegister),
        _ => Err(TranslateError::Unsupported {
            opcode: instr.opcode,
            reason: "unsupported mov operand combination",
        }),
    }
}

fn translate_lea(instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
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
        Some(X64AddrBase::Rip) => return Err(TranslateError::RipRelativeNeedsAddressComputation),
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

/// `add`/`sub`/`xor`: x64's destructive 2-operand form (`op dst, src` means
/// `dst = dst op src`) becomes ARM64's non-destructive 3-operand form
/// (`op dst, dst, src`) — same `dst` used twice. Register-only for now:
/// ARM64 has no memory-operand arithmetic, so a memory operand on either
/// side needs an explicit load first, which this doesn't do yet.
fn translate_add_sub(
    instr: &Instruction,
    arm_op: Arm64Opcode,
) -> Result<Vec<Instruction>, TranslateError> {
    let [dst, src] = take2(&instr.operands);

    let OperandKind::X64(X64OperandKind::Register(d)) = &dst.kind else {
        return Err(TranslateError::MemoryOperandNeedsScratchRegister);
    };
    let (dr, dw) = map_register_operand(*d)?;

    let src_operand = match &src.kind {
        OperandKind::X64(X64OperandKind::Register(s)) => {
            let (sr, _) = map_register_operand(*s)?;
            reg_operand(sr, dw, Role::Src)
        }
        OperandKind::X64(X64OperandKind::Immediate(n)) => imm_operand(*n, dw, Role::Src),
        OperandKind::X64(X64OperandKind::Memory(_)) => {
            return Err(TranslateError::MemoryOperandNeedsScratchRegister);
        }
        _ => {
            return Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "unsupported source operand",
            });
        }
    };

    Ok(vec![arm64_instr(
        arm_op,
        vec![
            reg_operand(dr, dw, Role::Dest),
            reg_operand(dr, dw, Role::Src),
            src_operand,
        ],
    )])
}

/// `cmp`/`test`: both operands are read-only (`Role::Src`) on both ISAs,
/// so unlike add/sub there's no destructive-vs-non-destructive shape
/// mismatch to resolve — the operand list carries over directly.
fn translate_cmp_test(
    instr: &Instruction,
    arm_op: Arm64Opcode,
) -> Result<Vec<Instruction>, TranslateError> {
    let [a, b] = take2(&instr.operands);

    let OperandKind::X64(X64OperandKind::Register(ar)) = &a.kind else {
        return Err(TranslateError::MemoryOperandNeedsScratchRegister);
    };
    let (ar, aw) = map_register_operand(*ar)?;

    let b_operand = match &b.kind {
        OperandKind::X64(X64OperandKind::Register(br)) => {
            let (br, _) = map_register_operand(*br)?;
            reg_operand(br, aw, Role::Src)
        }
        OperandKind::X64(X64OperandKind::Immediate(n)) => imm_operand(*n, aw, Role::Src),
        OperandKind::X64(X64OperandKind::Memory(_)) => {
            return Err(TranslateError::MemoryOperandNeedsScratchRegister);
        }
        _ => {
            return Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "unsupported operand",
            });
        }
    };

    Ok(vec![arm64_instr(
        arm_op,
        vec![reg_operand(ar, aw, Role::Src), b_operand],
    )])
}

/// x64 `inc`/`dec` have no immediate operand at all; ARM64 has no
/// dedicated increment instruction either, so both become `add`/`sub
/// dst, dst, #1` — the same destructive-to-non-destructive reshaping as
/// `translate_add_sub`, just with a synthesized immediate.
///
/// Known gap: x64 `inc`/`dec` famously leave the carry flag untouched
/// (unlike `add`/`sub`, which do set it) — since flags aren't modeled at
/// all yet, that divergence is invisible here, but it's a real
/// correctness issue once you do model flags.
fn translate_inc_dec(
    instr: &Instruction,
    arm_op: Arm64Opcode,
) -> Result<Vec<Instruction>, TranslateError> {
    let [dst] = take1(&instr.operands);

    let OperandKind::X64(X64OperandKind::Register(d)) = &dst.kind else {
        return Err(TranslateError::MemoryOperandNeedsScratchRegister);
    };
    let (dr, dw) = map_register_operand(*d)?;

    Ok(vec![arm64_instr(
        arm_op,
        vec![
            reg_operand(dr, dw, Role::Dest),
            reg_operand(dr, dw, Role::Src),
            imm_operand(1, dw, Role::Src),
        ],
    )])
}

/// `push reg` -> `str reg, [sp, #-8]!` (pre-indexed: decrement sp *first*,
/// then store). One ARM64 instruction, since `str` supports writeback
/// directly. Register operand only for now (x64 can push an immediate or
/// a memory operand too; both need an extra step to materialize a value
/// into a register first).
///
/// Known gap: this doesn't enforce ARM64's 16-byte stack-alignment
/// convention — a single 8-byte push, translated 1:1, will leave `sp`
/// misaligned relative to what AAPCS64 expects at a call boundary.
fn translate_push(instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
    let [src] = take1(&instr.operands);

    let OperandKind::X64(X64OperandKind::Register(s)) = &src.kind else {
        return Err(TranslateError::Unsupported {
            opcode: instr.opcode,
            reason: "only register operands supported for push",
        });
    };
    let (sr, sw) = map_register_operand(*s)?;

    let mem = Arm64MemOperand {
        base: Arm64Reg::Sp,
        offset: Some(-8),
        index: None,
        modifier: Arm64Modifier::None,
        pre_indexed: true,
        post_indexed: false,
    };
    Ok(vec![arm64_instr(
        Arm64Opcode::Str,
        vec![
            mem_operand(mem, sw, Role::Dest),
            reg_operand(sr, sw, Role::Src),
        ],
    )])
}

/// `pop reg` -> `ldr reg, [sp], #8` (post-indexed: load *first*, then
/// increment sp). Same stack-alignment caveat as `translate_push`.
fn translate_pop(instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
    let [dst] = take1(&instr.operands);

    let OperandKind::X64(X64OperandKind::Register(d)) = &dst.kind else {
        return Err(TranslateError::Unsupported {
            opcode: instr.opcode,
            reason: "only register operands supported for pop",
        });
    };
    let (dr, dw) = map_register_operand(*d)?;

    let mem = Arm64MemOperand {
        base: Arm64Reg::Sp,
        offset: Some(8),
        index: None,
        modifier: Arm64Modifier::None,
        pre_indexed: false,
        post_indexed: true,
    };
    Ok(vec![arm64_instr(
        Arm64Opcode::Ldr,
        vec![
            reg_operand(dr, dw, Role::Dest),
            mem_operand(mem, dw, Role::Src),
        ],
    )])
}

/// `ret` -> `ret`. Structurally 1:1 (no operands either side) but
/// **semantically not equivalent in isolation**: x64's `call`/`ret` pair
/// use the hardware stack for the return address; ARM64's `bl`/`ret` pair
/// use the link register (`x30`) instead and never touch the stack for
/// this by themselves. A translated function that itself calls other
/// functions needs an explicit `x30` save/restore in its prologue/epilogue
/// to avoid clobbering it — a leaf function (calls nothing) is fine
/// without it, but that's a whole-function property this instruction-level
/// translation can't see.
fn translate_ret(_instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
    Ok(vec![arm64_instr(Arm64Opcode::Ret, vec![])])
}

// ============================================================
// small local helpers
// ============================================================

fn take1(ops: &[Operand]) -> [&Operand; 1] {
    [&ops[0]]
}

fn take2(ops: &[Operand]) -> [&Operand; 2] {
    [&ops[0], &ops[1]]
}
