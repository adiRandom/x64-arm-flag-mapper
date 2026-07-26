use std::collections::HashMap;

use crate::input::ast::Line;
use crate::translator::{
    arm_modifiers::{Arm64Modifier, ShiftKind},
    instruction::{Arch, Instruction},
    loader::{self, LoaderError},
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
        }
    }
}

const SCRATCH_CANDIDATE_GPRS: &[Arm64Reg] = &[
    Arm64Reg::X(9),  // rax
    Arm64Reg::X(19), // rbx
    Arm64Reg::X(3),  // rcx
    Arm64Reg::X(2),  // rdx
    Arm64Reg::X(1),  // rsi
    Arm64Reg::X(0),  // rdi
    Arm64Reg::X(29), // rbp
    Arm64Reg::X(4),  // r8
    Arm64Reg::X(5),  // r9
    Arm64Reg::X(10), // r10
    Arm64Reg::X(11), // r11
    Arm64Reg::X(20), // r12
    Arm64Reg::X(21), // r13
    Arm64Reg::X(22), // r14
    Arm64Reg::X(23), // r15
];

pub struct Translator {
    reg_last_used: HashMap<Arm64Reg, usize>,
    current_x86_idx: usize,
}

impl Translator {
    pub fn new() -> Self {
        Self {
            reg_last_used: HashMap::new(),
            current_x86_idx: 0,
        }
    }

    pub fn load_program(&self, lines: &[Line]) -> Result<Vec<Instruction>, LoaderError> {
        loader::load_program(lines)
    }

    /// Translates a loaded x86 instruction slice into ARM64 using a
    /// two-pass scratch-register allocation strategy.
    ///
    /// **Pass 1 — forward translation.**
    /// Instructions are processed left-to-right. Register uses are
    /// recorded incrementally in `reg_last_used`. Each time a scratch
    /// register is needed (ARM64 has no memory-operand arithmetic or
    /// store-immediate), the allocator picks the first GPR that has
    /// *never* been seen (`reg_last_used` has no entry for it). If no
    /// clean register exists, an [`Arm64Reg::Placeholder`] carrying the
    /// current x86 instruction index is emitted instead.
    ///
    /// **Pass 2 — placeholder resolution.**
    /// After pass 1, `reg_last_used` reflects the *last* use of every
    /// GPR across the entire input. For each placeholder created at x86
    /// index `p`, the resolver searches for a GPR whose `last_used < p` —
    /// meaning it was last touched *before* instruction `p` and is
    /// therefore dead at that point. If one is found it replaces the
    /// placeholder. If no dead register exists, the translated group is
    /// wrapped with a push/pop spill sequence using any register that was
    /// not an operand of the original x86 instruction.
    pub fn translate_program(
        &mut self,
        instrs: &[Instruction],
    ) -> Result<Vec<Instruction>, TranslateError> {
        // Each entry: (x86_idx, Vec<arm64_instructions>)
        let mut groups: Vec<(usize, Vec<Instruction>)> = Vec::with_capacity(instrs.len());

        // Pass 1: translate, tracking register usage along the way.
        for (x86_idx, instr) in instrs.iter().enumerate() {
            self.current_x86_idx = x86_idx;
            self.record_uses(instr);
            let arm_instrs = instr.to_arm64(self)?;
            groups.push((x86_idx, arm_instrs));
        }

        // Pass 2: resolve placeholders now that reg_last_used is complete.
        self.resolve_placeholders(groups)
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Records every ARM64 GPR (after mapping from x64) referenced by
    /// `instr` as "last used at `current_x86_idx`".
    fn record_uses(&mut self, instr: &Instruction) {
        let idx = self.current_x86_idx;
        for op in &instr.operands {
            match &op.kind {
                OperandKind::X64(X64OperandKind::Register(X64Reg::Gpr(gpr, _))) => {
                    self.reg_last_used.insert(map_gpr(*gpr), idx);
                }
                OperandKind::X64(X64OperandKind::Memory(m)) => {
                    if let Some(X64AddrBase::Reg(gpr)) = m.base {
                        self.reg_last_used.insert(map_gpr(gpr), idx);
                    }
                    if let Some(gpr) = m.index {
                        self.reg_last_used.insert(map_gpr(gpr), idx);
                    }
                }
                _ => {}
            }
        }
    }

    /// Returns the first ARM64 GPR candidate that has never appeared in
    /// the instruction stream so far (a "clean" register), or
    /// [`Arm64Reg::Placeholder`] carrying `current_x86_idx` if every
    /// candidate has been used at least once.
    fn alloc_scratch(&self) -> Arm64Reg {
        for &reg in SCRATCH_CANDIDATE_GPRS {
            if !self.reg_last_used.contains_key(&reg) {
                return reg;
            }
        }
        Arm64Reg::Placeholder(self.current_x86_idx as u32)
    }

    /// Resolves all [`Arm64Reg::Placeholder`] entries in `groups`,
    /// producing a flat `Vec<Instruction>`.
    fn resolve_placeholders(
        &self,
        groups: Vec<(usize, Vec<Instruction>)>,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let mut output = Vec::new();

        for (x86_idx, mut arm_instrs) in groups {
            if !group_has_placeholder(&arm_instrs) {
                output.extend(arm_instrs);
                continue;
            }

            // Try to find an ARM64 GPR that was last used strictly before
            // x86_idx (dead at x86_idx and beyond).
            let dead_scratch = SCRATCH_CANDIDATE_GPRS.iter().find(|&&reg| {
                match self.reg_last_used.get(&reg) {
                    Some(&last) => last < x86_idx,
                    // Never used — should have been caught in pass 1, but
                    // treat it as available here too.
                    None => true,
                }
            });

            if let Some(&scratch) = dead_scratch {
                // Substitute the placeholder with this dead register.
                for instr in &mut arm_instrs {
                    replace_placeholder(&mut instr.operands, scratch);
                }
                output.extend(arm_instrs);
            } else {
                // No dead register — spill one to the stack.
                // Choose any register that is not an operand of the x86
                // instruction at x86_idx (last_used != x86_idx) so we
                // don't accidentally clobber a live value mid-instruction.
                let spill_reg = SCRATCH_CANDIDATE_GPRS
                    .iter()
                    .find(|&&reg| {
                        self.reg_last_used.get(&reg).copied().unwrap_or(usize::MAX) != x86_idx
                    })
                    .copied()
                    .expect("at least one ARM64 GPR is not an operand of the current instruction");

                // Replace placeholders with the chosen spill register.
                for instr in &mut arm_instrs {
                    replace_placeholder(&mut instr.operands, spill_reg);
                }

                // Bracket the group with a push/pop pair to preserve the
                // spilled register's value across the scratch use.
                output.push(make_push(spill_reg));
                output.extend(arm_instrs);
                output.push(make_pop(spill_reg));
            }
        }

        Ok(output)
    }
}

impl Default for Translator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// Placeholder scanning / replacement helpers
// ============================================================

fn group_has_placeholder(instrs: &[Instruction]) -> bool {
    instrs
        .iter()
        .any(|i| operands_have_placeholder(&i.operands))
}

fn operands_have_placeholder(ops: &[Operand]) -> bool {
    ops.iter().any(operand_has_placeholder)
}

fn operand_has_placeholder(op: &Operand) -> bool {
    // Placeholders only appear as Register operands today — alloc_scratch()
    // is always passed to reg_operand(), never used as a memory base/index.
    // When CombinedIndexAndDisplacement is eventually handled by emitting
    // `add scratch, base, #disp` + `[scratch, index, lsl #s]`, the scratch
    // *will* end up as a memory base and this check will need extending.
    // At that point Placeholder will also need a per-slot discriminator
    // (e.g. Placeholder { x86_idx, slot }) so two allocations within the
    // same instruction can be resolved to two distinct registers.
    matches!(
        op.kind,
        OperandKind::Arm64(Arm64OperandKind::Register(Arm64Reg::Placeholder(_), _))
    )
}

fn replace_placeholder(ops: &mut [Operand], replacement: Arm64Reg) {
    for op in ops {
        if let OperandKind::Arm64(Arm64OperandKind::Register(reg, _)) = &mut op.kind {
            if matches!(reg, Arm64Reg::Placeholder(_)) {
                *reg = replacement;
            }
        }
    }
}

// Push/pop helpers for the spill fallback path.
fn make_push(reg: Arm64Reg) -> Instruction {
    arm64_instr(
        Arm64Opcode::Str,
        vec![
            mem_operand(
                Arm64MemOperand {
                    base: Arm64Reg::Sp,
                    offset: Some(-8),
                    index: None,
                    modifier: Arm64Modifier::None,
                    pre_indexed: true,
                    post_indexed: false,
                },
                Width::W64,
                Role::Dest,
            ),
            reg_operand(reg, Width::W64, Role::Src),
        ],
    )
}

fn make_pop(reg: Arm64Reg) -> Instruction {
    arm64_instr(
        Arm64Opcode::Ldr,
        vec![
            reg_operand(reg, Width::W64, Role::Dest),
            mem_operand(
                Arm64MemOperand {
                    base: Arm64Reg::Sp,
                    offset: Some(8),
                    index: None,
                    modifier: Arm64Modifier::None,
                    pre_indexed: false,
                    post_indexed: true,
                },
                Width::W64,
                Role::Src,
            ),
        ],
    )
}

// ============================================================
// Instruction::to_arm64
// ============================================================

impl Instruction {
    /// Translates one x64 instruction into zero or more ARM64 instructions,
    /// using `translator` for scratch-register allocation.
    ///
    /// Some x64 instructions expand to multiple ARM64 instructions
    /// (`push`/`pop` are still 1:1, but memory-operand arithmetic expands
    /// to a load, the operation, and a store) — hence `Vec`.
    pub fn to_arm64(
        &self,
        translator: &mut Translator,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let x64_op = match self.opcode {
            Opcode::X64(op) => op,
            Opcode::Arm64(_) => return Err(TranslateError::AlreadyArm64),
        };

        match x64_op {
            X64Opcode::Mov => translate_mov(self, translator),
            X64Opcode::Lea => translate_lea(self, translator),
            X64Opcode::Add => translate_add_sub(self, Arm64Opcode::Add, translator),
            X64Opcode::Sub => translate_add_sub(self, Arm64Opcode::Sub, translator),
            X64Opcode::Xor => translate_add_sub(self, Arm64Opcode::Eor, translator),
            X64Opcode::Cmp => translate_cmp_test(self, Arm64Opcode::Cmp, translator),
            X64Opcode::Test => translate_cmp_test(self, Arm64Opcode::Tst, translator),
            X64Opcode::Inc => translate_inc_dec(self, Arm64Opcode::Add, translator),
            X64Opcode::Dec => translate_inc_dec(self, Arm64Opcode::Sub, translator),
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

fn translate_mov(
    instr: &Instruction,
    translator: &mut Translator,
) -> Result<Vec<Instruction>, TranslateError> {
    let [dest, src] = take2(&instr.operands);

    match (&dest.kind, &src.kind) {
        // reg <- reg
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
        // reg <- [mem]
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
        // [mem] <- reg
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
        // reg <- imm
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
        // [mem] <- imm: ARM64 has no store-immediate — materialise the
        // immediate in a scratch register, then store that register.
        (
            OperandKind::X64(X64OperandKind::Memory(m)),
            OperandKind::X64(X64OperandKind::Immediate(n)),
        ) => {
            let scratch = translator.alloc_scratch();
            let am = map_mem_operand(m)?;
            let width = dest.width;
            Ok(vec![
                arm64_instr(
                    Arm64Opcode::Mov,
                    vec![
                        reg_operand(scratch, width, Role::Dest),
                        imm_operand(*n, width, Role::Src),
                    ],
                ),
                arm64_instr(
                    Arm64Opcode::Str,
                    vec![
                        mem_operand(am, width, Role::Dest),
                        reg_operand(scratch, width, Role::Src),
                    ],
                ),
            ])
        }
        _ => Err(TranslateError::Unsupported {
            opcode: instr.opcode,
            reason: "unsupported mov operand combination",
        }),
    }
}

fn translate_lea(
    instr: &Instruction,
    _translator: &mut Translator,
) -> Result<Vec<Instruction>, TranslateError> {
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
/// `dst = dst op src`) becomes ARM64's non-destructive 3-operand form.
///
/// When either operand is a memory location, a scratch register is used to
/// load/store through. For a memory destination the sequence is:
/// `ldr scratch, [mem]; op scratch, scratch, src; str scratch, [mem]`.
/// For a memory source: `ldr scratch, [mem]; op dst, dst, scratch`.
fn translate_add_sub(
    instr: &Instruction,
    arm_op: Arm64Opcode,
    translator: &mut Translator,
) -> Result<Vec<Instruction>, TranslateError> {
    let [dst, src] = take2(&instr.operands);

    match (&dst.kind, &src.kind) {
        // reg op= reg
        (
            OperandKind::X64(X64OperandKind::Register(d)),
            OperandKind::X64(X64OperandKind::Register(s)),
        ) => {
            let (dr, dw) = map_register_operand(*d)?;
            let (sr, _) = map_register_operand(*s)?;
            Ok(vec![arm64_instr(
                arm_op,
                vec![
                    reg_operand(dr, dw, Role::Dest),
                    reg_operand(dr, dw, Role::Src),
                    reg_operand(sr, dw, Role::Src),
                ],
            )])
        }
        // reg op= imm
        (
            OperandKind::X64(X64OperandKind::Register(d)),
            OperandKind::X64(X64OperandKind::Immediate(n)),
        ) => {
            let (dr, dw) = map_register_operand(*d)?;
            Ok(vec![arm64_instr(
                arm_op,
                vec![
                    reg_operand(dr, dw, Role::Dest),
                    reg_operand(dr, dw, Role::Src),
                    imm_operand(*n, dw, Role::Src),
                ],
            )])
        }
        // reg op= [mem]: load memory into scratch, then operate.
        (
            OperandKind::X64(X64OperandKind::Register(d)),
            OperandKind::X64(X64OperandKind::Memory(m)),
        ) => {
            let (dr, dw) = map_register_operand(*d)?;
            let scratch = translator.alloc_scratch();
            let am = map_mem_operand(m)?;
            Ok(vec![
                arm64_instr(
                    Arm64Opcode::Ldr,
                    vec![
                        reg_operand(scratch, dw, Role::Dest),
                        mem_operand(am, dw, Role::Src),
                    ],
                ),
                arm64_instr(
                    arm_op,
                    vec![
                        reg_operand(dr, dw, Role::Dest),
                        reg_operand(dr, dw, Role::Src),
                        reg_operand(scratch, dw, Role::Src),
                    ],
                ),
            ])
        }
        // [mem] op= reg: load, operate, store.
        (
            OperandKind::X64(X64OperandKind::Memory(m)),
            OperandKind::X64(X64OperandKind::Register(s)),
        ) => {
            let (sr, sw) = map_register_operand(*s)?;
            let scratch = translator.alloc_scratch();
            let am = map_mem_operand(m)?;
            Ok(vec![
                arm64_instr(
                    Arm64Opcode::Ldr,
                    vec![
                        reg_operand(scratch, sw, Role::Dest),
                        mem_operand(am, sw, Role::Src),
                    ],
                ),
                arm64_instr(
                    arm_op,
                    vec![
                        reg_operand(scratch, sw, Role::Dest),
                        reg_operand(scratch, sw, Role::Src),
                        reg_operand(sr, sw, Role::Src),
                    ],
                ),
                arm64_instr(
                    Arm64Opcode::Str,
                    vec![
                        mem_operand(am, sw, Role::Dest),
                        reg_operand(scratch, sw, Role::Src),
                    ],
                ),
            ])
        }
        // [mem] op= imm: load, operate, store.
        (
            OperandKind::X64(X64OperandKind::Memory(m)),
            OperandKind::X64(X64OperandKind::Immediate(n)),
        ) => {
            let width = dst.width;
            let scratch = translator.alloc_scratch();
            let am = map_mem_operand(m)?;
            Ok(vec![
                arm64_instr(
                    Arm64Opcode::Ldr,
                    vec![
                        reg_operand(scratch, width, Role::Dest),
                        mem_operand(am, width, Role::Src),
                    ],
                ),
                arm64_instr(
                    arm_op,
                    vec![
                        reg_operand(scratch, width, Role::Dest),
                        reg_operand(scratch, width, Role::Src),
                        imm_operand(*n, width, Role::Src),
                    ],
                ),
                arm64_instr(
                    Arm64Opcode::Str,
                    vec![
                        mem_operand(am, width, Role::Dest),
                        reg_operand(scratch, width, Role::Src),
                    ],
                ),
            ])
        }
        _ => Err(TranslateError::Unsupported {
            opcode: instr.opcode,
            reason: "unsupported source operand",
        }),
    }
}

/// `cmp`/`test`: both operands are read-only on both ISAs.
/// A memory operand on either side is loaded into a scratch register first.
fn translate_cmp_test(
    instr: &Instruction,
    arm_op: Arm64Opcode,
    translator: &mut Translator,
) -> Result<Vec<Instruction>, TranslateError> {
    let [a, b] = take2(&instr.operands);

    // Resolve the first operand (may be a register or memory).
    let (a_reg, a_width, a_pre) = match &a.kind {
        OperandKind::X64(X64OperandKind::Register(ar)) => {
            let (ar, aw) = map_register_operand(*ar)?;
            (ar, aw, vec![])
        }
        OperandKind::X64(X64OperandKind::Memory(m)) => {
            let scratch = translator.alloc_scratch();
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
            let scratch = translator.alloc_scratch();
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

/// x64 `inc`/`dec` have no immediate operand at all; ARM64 has no dedicated
/// increment instruction either, so both become `add`/`sub dst, dst, #1`.
///
/// When the operand is a memory location, the sequence is:
/// `ldr scratch, [mem]; add/sub scratch, scratch, #1; str scratch, [mem]`.
///
/// Known gap: x64 `inc`/`dec` leave the carry flag untouched (unlike
/// `add`/`sub`) — since flags aren't modeled yet, that divergence is
/// invisible here.
fn translate_inc_dec(
    instr: &Instruction,
    arm_op: Arm64Opcode,
    translator: &mut Translator,
) -> Result<Vec<Instruction>, TranslateError> {
    let [dst] = take1(&instr.operands);

    match &dst.kind {
        OperandKind::X64(X64OperandKind::Register(d)) => {
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
        OperandKind::X64(X64OperandKind::Memory(m)) => {
            let scratch = translator.alloc_scratch();
            let am = map_mem_operand(m)?;
            let width = dst.width;
            Ok(vec![
                arm64_instr(
                    Arm64Opcode::Ldr,
                    vec![
                        reg_operand(scratch, width, Role::Dest),
                        mem_operand(am, width, Role::Src),
                    ],
                ),
                arm64_instr(
                    arm_op,
                    vec![
                        reg_operand(scratch, width, Role::Dest),
                        reg_operand(scratch, width, Role::Src),
                        imm_operand(1, width, Role::Src),
                    ],
                ),
                arm64_instr(
                    Arm64Opcode::Str,
                    vec![
                        mem_operand(am, width, Role::Dest),
                        reg_operand(scratch, width, Role::Src),
                    ],
                ),
            ])
        }
        _ => Err(TranslateError::Unsupported {
            opcode: instr.opcode,
            reason: "unsupported operand for inc/dec",
        }),
    }
}

/// `push reg` -> `str reg, [sp, #-8]!` (pre-indexed: decrement sp *first*,
/// then store). Register operand only for now.
///
/// Known gap: this doesn't enforce ARM64's 16-byte stack-alignment
/// convention — a single 8-byte push leaves `sp` misaligned relative to
/// what AAPCS64 expects at a call boundary.
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

/// `ret` -> `ret`. Semantically not equivalent in isolation: x64's
/// `call`/`ret` pair uses the hardware stack; ARM64's `bl`/`ret` uses the
/// link register (`x30`). A translated function that itself calls other
/// functions needs an explicit `x30` save/restore in its prologue/epilogue.
fn translate_ret(_instr: &Instruction) -> Result<Vec<Instruction>, TranslateError> {
    Ok(vec![arm64_instr(Arm64Opcode::Ret, vec![])])
}

// ============================================================
// Small local helpers
// ============================================================

fn take1(ops: &[Operand]) -> [&Operand; 1] {
    [&ops[0]]
}

fn take2(ops: &[Operand]) -> [&Operand; 2] {
    [&ops[0], &ops[1]]
}
