use std::collections::{HashMap, HashSet};

use crate::input::ast::Line;
use crate::translator::{
    arm_modifiers::Arm64Modifier,
    directive_translator::translate_directive,
    flags::FLAG_BITS,
    instruction::Instruction,
    loader::{self, LoaderError},
    opcodes::{Arm64Opcode, Opcode, X64Opcode},
    operand::{
        Arm64MemOperand, Arm64OperandKind, Operand, OperandKind, Role, X64AddrBase, X64OperandKind,
    },
    register::{Arm64Reg, X64GpReg, X64GpSlice, X64Reg},
    statement::TranslationStatement,
    util::{Width, arm64_instr, map_gpr, mem_operand, reg_operand},
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
    src_program: Vec<TranslationStatement>,
    pub translated_program: Vec<TranslationStatement>,
    /// The x64 line index of the instruction that last wrote each flag.
    /// Indexed by position in `FLAG_BITS`: 0=CF, 1=PF, 2=AF, 3=ZF, 4=SF, 5=OF.
    last_flag_writer: [Option<usize>; 6],
    /// x64 line indices whose translated ARM64 group must have `produces_flags`
    /// toggled on at least one instruction so that a following `B.cond` can
    /// read the correct NZCV state.
    flag_producer_indices: HashSet<usize>,
}

impl Translator {
    pub fn new() -> Self {
        Self {
            reg_last_used: HashMap::new(),
            current_x86_idx: 0,
            src_program: Vec::new(),
            translated_program: Vec::new(),
            last_flag_writer: [None; 6],
            flag_producer_indices: HashSet::new(),
        }
    }

    pub fn load_program(&mut self, lines: &[Line]) -> Option<LoaderError> {
        self.src_program = loader::load_program(lines).ok()?;
        None
    }

    fn map_instruction_to_arm64(
        &self,
        instr: &Instruction,
    ) -> Result<Vec<Instruction>, TranslateError> {
        let x64_op = match instr.opcode {
            Opcode::X64(op) => op,
            Opcode::Arm64(_) => return Err(TranslateError::AlreadyArm64),
        };

        match x64_op {
            X64Opcode::Mov => self.translate_mov(instr),
            X64Opcode::Lea => self.translate_lea(instr),
            X64Opcode::Add => self.translate_add_sub(instr, Arm64Opcode::Add),
            X64Opcode::Sub => self.translate_add_sub(instr, Arm64Opcode::Sub),
            X64Opcode::Xor => self.translate_add_sub(instr, Arm64Opcode::Eor),
            X64Opcode::Cmp => self.translate_cmp_test(instr, Arm64Opcode::Cmp),
            X64Opcode::Test => self.translate_cmp_test(instr, Arm64Opcode::Tst),
            X64Opcode::Inc => self.translate_inc_dec(instr, Arm64Opcode::Add),
            X64Opcode::Dec => self.translate_inc_dec(instr, Arm64Opcode::Sub),
            X64Opcode::Push => self.translate_push(instr),
            X64Opcode::Pop => self.translate_pop(instr),
            X64Opcode::Ret => self.translate_ret(instr),
            X64Opcode::Jmp => self.translate_jmp(instr),
            X64Opcode::Jcc(cond) => self.translate_jcc(instr, cond),
            X64Opcode::Call => self.translate_call(instr),
            X64Opcode::Mul => Err(TranslateError::Unsupported {
                opcode: instr.opcode,
                reason: "implicit rdx:rax destination isn't modeled as an operand yet",
            }),
        }
    }

    fn translation_cleanup(&mut self) {
        self.translated_program.clear();
        self.reg_last_used.clear();
        self.current_x86_idx = 0;
        self.last_flag_writer = [None; 6];
        self.flag_producer_indices.clear();
    }

    fn translate_instruction(
        &mut self,
        instr: &Instruction,
    ) -> Result<Vec<Instruction>, TranslateError> {
        self.record_uses(instr);
        self.record_flags(instr);
        self.map_instruction_to_arm64(instr)
    }

    /// Translates a loaded x86 instruction slice into ARM64 using a
    /// two-pass scratch-register allocation strategy plus a flag-production pass.
    ///
    /// **Pass 1 — forward translation.**
    /// Instructions are processed left-to-right. Register uses are
    /// recorded incrementally in `reg_last_used`. Each time a scratch
    /// register is needed (ARM64 has no memory-operand arithmetic or
    /// store-immediate), the allocator picks the first GPR that has
    /// *never* been seen (`reg_last_used` has no entry for it). If no
    /// clean register exists, an [`Arm64Reg::Placeholder`] carrying the
    /// current x86 instruction index is emitted instead.
    /// Concurrently, `record_flags` maintains `last_flag_writer` so that
    /// when a flag-reading instruction (e.g. `jge`) is encountered, the
    /// x64 index of the instruction that last wrote each needed flag is
    /// pushed into `flag_producer_indices`.
    ///
    /// **Pass 2 — placeholder resolution + flag production.**
    /// After pass 1, `reg_last_used` reflects the *last* use of every
    /// GPR across the entire input. For each placeholder created at x86
    /// index `p`, the resolver searches for a GPR whose `last_used < p` —
    /// meaning it was last touched *before* instruction `p` and is
    /// therefore dead at that point. If one is found it replaces the
    /// placeholder. If no dead register exists, the translated group is
    /// wrapped with a push/pop spill sequence using any register that was
    /// not an operand of the original x86 instruction.
    /// The flag-production sub-pass then walks `flag_producer_indices` and
    /// toggles `produces_flags = true` on the appropriate ARM64 instruction
    /// in each group, causing the emitter to use the `S`-suffix variant
    /// (e.g. `adds`, `subs`) so that NZCV flags are visible to `B.cond`.
    pub fn translate_program(&mut self) -> Option<TranslateError> {
        self.translation_cleanup();

        let src_program = self.src_program.clone();

        // Pass 1: translate, tracking register usage along the way.
        let translation_results = src_program
            .iter()
            .enumerate()
            .map(
                |(x86_idx, statement)| -> Result<Vec<TranslationStatement>, TranslateError> {
                    self.current_x86_idx = x86_idx;

                    match statement {
                        TranslationStatement::Instruction(instr, idx) => {
                            let arm_instrs = self.translate_instruction(instr)?;
                            let translated_statements = arm_instrs
                                .into_iter()
                                .map(|translated_instr| {
                                    TranslationStatement::Instruction(translated_instr, idx.clone())
                                })
                                .collect::<Vec<_>>();
                            Ok(translated_statements)
                        }
                        TranslationStatement::Label(_) => Ok(vec![statement.clone()]),
                        TranslationStatement::Directive(d) => Ok(translate_directive(d)
                            .map(|translated| vec![TranslationStatement::Directive(translated)])
                            .unwrap_or_default()),
                    }
                },
            )
            .collect::<Result<Vec<_>, _>>();

        if let Err(err) = translation_results {
            return Some(err);
        }

        self.translated_program = translation_results
            .iter()
            .flatten()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();

        // Group ARM64 indices by their x64 index. No instruction data needed
        // here — just indices — so no cloning of translated_program at all.
        let mut x64_to_arm_idx_grouping: HashMap<usize, Vec<usize>> = HashMap::new();
        for (arm_idx, statement) in self.translated_program.iter().enumerate() {
            if let TranslationStatement::Instruction(_, x64_idx) = statement {
                x64_to_arm_idx_grouping.entry(*x64_idx).or_default().push(arm_idx);
            }
        }

        // Pass 2: resolve placeholders now that reg_last_used is complete,
        // then toggle flag-producing instructions based on flag_producer_indices.
        self.resolve_placeholders(&x64_to_arm_idx_grouping);
        self.flag_production_pass(&x64_to_arm_idx_grouping);

        None
    }

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

    /// Updates `last_flag_writer` and `flag_producer_indices` for one x64 instruction.
    ///
    /// Called during pass 1 alongside `record_uses`.  The read-before-write
    /// order is intentional: if an instruction reads and writes the same flag
    /// (e.g. a hypothetical ADC), we resolve the read against the *previous*
    /// writer, not the current instruction itself.
    fn record_flags(&mut self, instr: &Instruction) {
        let x64_idx = self.current_x86_idx;

        // 1. Reads: find which earlier instruction last wrote each flag we read,
        //    and mark it as needing to produce ARM64 NZCV flags.
        for (i, &flag) in FLAG_BITS.iter().enumerate() {
            if instr.flags_read.contains(flag) {
                if let Some(writer_idx) = self.last_flag_writer[i] {
                    self.flag_producer_indices.insert(writer_idx);
                }
                // If last_flag_writer[i] is None the flag was never set in
                // this translation unit (e.g. set by the caller before the call).
                // We can't track that here so we skip it.
            }
        }

        // 2. Writes: record this instruction as the most recent writer of
        //    each flag it produces.
        for (i, &flag) in FLAG_BITS.iter().enumerate() {
            if instr.flags_written.contains(flag) {
                self.last_flag_writer[i] = Some(x64_idx);
            }
        }
    }

    /// Pass 2 sub-pass: for every x64 group in `flag_producer_indices`, find
    /// the ARM64 instruction(s) in that group that have a flag-setting variant
    /// and toggle `produces_flags = true` on them.
    fn flag_production_pass(&mut self, idx_groups: &HashMap<usize, Vec<usize>>) {
        // Collect which ARM64 instruction indices need toggling.
        // Done in a separate read pass to avoid simultaneous mutable and
        // immutable borrows of `translated_program`.
        let mut to_toggle: Vec<usize> = Vec::new();

        for &x64_idx in &self.flag_producer_indices {
            let Some(arm_indices) = idx_groups.get(&x64_idx) else {
                continue;
            };

            // ARM instructions that can be given an S-suffix (adds / subs / eors).
            let toggleable: Vec<usize> = arm_indices
                .iter()
                .copied()
                .filter(|&i| {
                    matches!(
                        &self.translated_program[i],
                        TranslationStatement::Instruction(instr, _)
                            if can_toggle_flag_production(instr)
                    )
                })
                .collect();

            // ARM instructions that always set flags (cmp / tst) — no toggle
            // needed; they already satisfy the requirement.
            let has_inherent = arm_indices.iter().any(|&i| {
                matches!(
                    &self.translated_program[i],
                    TranslationStatement::Instruction(instr, _)
                        if already_sets_flags(instr)
                )
            });

            if toggleable.is_empty() {
                if !has_inherent {
                    // TODO: flag emulation — no instruction in this group has a
                    // flag-setting variant (e.g. mov, ldr).  Needs software
                    // emulation to materialise the flag value in NZCV.
                }
                continue;
            }

            if toggleable.len() > 1 {
                // TODO: when multiple ARM64 instructions in a group are toggleable,
                // only the *last* one that writes the relevant flag should be
                // toggled.  For now, toggle all of them to be safe.
                to_toggle.extend_from_slice(&toggleable);
            } else {
                to_toggle.push(toggleable[0]);
            }
        }

        // Apply the toggles.
        for arm_idx in to_toggle {
            if let TranslationStatement::Instruction(instr, _) =
                &mut self.translated_program[arm_idx]
            {
                instr.produces_flags = true;
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

    fn resolve_placeholders(&mut self, idx_groups: &HashMap<usize, Vec<usize>>) {
        for (x86_idx, arm_indices) in idx_groups {
            // Scoped immutable borrow just to run the placeholder check —
            // it ends before we need to mutate translated_program.
            let has_placeholder = {
                let instructions: Vec<(&Instruction, usize)> = arm_indices
                    .iter()
                    .map(|&arm_idx| match &self.translated_program[arm_idx] {
                        TranslationStatement::Instruction(instr, _) => (instr, arm_idx),
                        _ => unreachable!("idx_groups only contains Instruction indices"),
                    })
                    .collect();
                group_has_placeholder(&instructions)
            };

            if !has_placeholder {
                continue;
            }

            // Try to find an ARM64 GPR that was last used strictly before
            // x86_idx (dead at x86_idx and beyond).
            let dead_scratch = SCRATCH_CANDIDATE_GPRS.iter().copied().find(|reg| {
                match self.reg_last_used.get(reg) {
                    Some(&last) => last < *x86_idx,
                    None => true,
                }
            });

            if let Some(scratch) = dead_scratch {
                for &arm_idx in arm_indices {
                    if let TranslationStatement::Instruction(instr, _) =
                        &mut self.translated_program[arm_idx]
                    {
                        instr.operands = resolve_placeholder_in_operands(&instr.operands, scratch);
                    }
                }
            } else {
                // No dead register — spill one to the stack. Pick any register
                // that isn't an operand of the x86 instruction at x86_idx.
                let spill_reg = SCRATCH_CANDIDATE_GPRS
                    .iter()
                    .copied()
                    .find(|reg| {
                        self.reg_last_used.get(reg).copied().unwrap_or(usize::MAX) != *x86_idx
                    })
                    .expect("at least one ARM64 GPR is not an operand of the current instruction");

                for &arm_idx in arm_indices {
                    if let TranslationStatement::Instruction(instr, _) =
                        &mut self.translated_program[arm_idx]
                    {
                        instr.operands =
                            resolve_placeholder_in_operands(&instr.operands, spill_reg);
                    }
                }

                let first_arm_idx = *arm_indices.first().unwrap_or(&0);
                let last_arm_idx = *arm_indices.last().unwrap_or(&0);

                self.translated_program.insert(
                    first_arm_idx,
                    TranslationStatement::Instruction(make_push(spill_reg), 0),
                );
                self.translated_program.insert(
                    last_arm_idx + 1,
                    TranslationStatement::Instruction(make_pop(spill_reg), 0),
                );
            }
        }
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

fn group_has_placeholder(instrs: &Vec<(&Instruction, usize)>) -> bool {
    instrs
        .iter()
        .any(|(i, _)| operands_have_placeholder(&i.operands))
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
        &op.kind,
        OperandKind::Arm64(Arm64OperandKind::Register(Arm64Reg::Placeholder(_), _))
    )
}

fn resolve_placeholder_in_operands(ops: &[Operand], replacement: Arm64Reg) -> Vec<Operand> {
    ops.iter()
        .map(|op| {
            let mut mapped = op.clone();

            if let OperandKind::Arm64(Arm64OperandKind::Register(reg, _)) = &mut mapped.kind {
                if matches!(reg, Arm64Reg::Placeholder(_)) {
                    *reg = replacement;
                }
            }

            mapped
        })
        .collect::<Vec<_>>()
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

/// Returns `true` if this ARM64 instruction can be switched to its
/// flag-setting variant (`add` → `adds`, `sub` → `subs`, `eor` → `eors`).
fn can_toggle_flag_production(instr: &Instruction) -> bool {
    matches!(
        instr.opcode,
        Opcode::Arm64(Arm64Opcode::Add | Arm64Opcode::Sub | Arm64Opcode::Eor)
    )
}

/// Returns `true` if this ARM64 instruction *inherently* sets NZCV flags
/// (`cmp` / `tst`) and therefore needs no toggling.
fn already_sets_flags(instr: &Instruction) -> bool {
    matches!(
        instr.opcode,
        Opcode::Arm64(Arm64Opcode::Cmp | Arm64Opcode::Tst)
    )
}
