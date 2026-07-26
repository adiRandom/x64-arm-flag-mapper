use std::collections::HashMap;

use crate::input::ast::Line;
use crate::translator::util::{arm64_instr, map_gpr, mem_operand, reg_operand};
use crate::translator::{
    arm_modifiers::Arm64Modifier,
    instruction::Instruction,
    loader::{self, LoaderError},
    opcodes::{Arm64Opcode, Opcode, X64Opcode},
    operand::{
        Arm64MemOperand, Arm64OperandKind, Operand, OperandKind, Role, X64AddrBase, X64OperandKind,
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
    pub(super) fn alloc_scratch(&self) -> Arm64Reg {
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
            X64Opcode::Mov => translator.translate_mov(self),
            X64Opcode::Lea => translator.translate_lea(self),
            X64Opcode::Add => translator.translate_add_sub(self, Arm64Opcode::Add),
            X64Opcode::Sub => translator.translate_add_sub(self, Arm64Opcode::Sub),
            X64Opcode::Xor => translator.translate_add_sub(self, Arm64Opcode::Eor),
            X64Opcode::Cmp => translator.translate_cmp_test(self, Arm64Opcode::Cmp),
            X64Opcode::Test => translator.translate_cmp_test(self, Arm64Opcode::Tst),
            X64Opcode::Inc => translator.translate_inc_dec(self, Arm64Opcode::Add),
            X64Opcode::Dec => translator.translate_inc_dec(self, Arm64Opcode::Sub),
            X64Opcode::Push => translator.translate_push(self),
            X64Opcode::Pop => translator.translate_pop(self),
            X64Opcode::Ret => translator.translate_ret(self),
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

// Per-opcode translation functions live in the instr_translator submodules.

// ============================================================
// Small local helpers
// ============================================================
