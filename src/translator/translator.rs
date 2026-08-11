use std::collections::HashMap;

use crate::input::ast::Line;
use crate::translator::instr_translator::translate_parity::emit_parity_for_x64_instr;
use crate::translator::{
    arm_modifiers::Arm64Modifier,
    cpu_info::{CPU_INFO_REG, CPU_INFO_SIZE},
    directive_translator::translate_directive,
    flags::{EMULATED_FLAGS, FlagSet, NZCV_FLAGS},
    instruction::Instruction,
    loader::{self, LoaderError},
    opcodes::{Arm64Opcode, Opcode, X64Opcode},
    operand::{
        Arm64MemOperand, Arm64OperandKind, Operand, OperandKind, Role, X64AddrBase, X64OperandKind,
    },
    register::{Arm64Reg, X64GpReg, X64GpSlice, X64Reg},
    statement::TranslationStatement,
    util::{Width, arm64_instr, imm_operand, map_gpr, mem_operand, reg_operand},
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

const SCRATCH_GPR: Arm64Reg = Arm64Reg::X(24);

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
    Arm64Reg::X(25),
    Arm64Reg::X(26),
    Arm64Reg::X(27),
];

pub struct Translator {
    reg_last_used: HashMap<Arm64Reg, usize>,
    current_x86_idx: usize,
    src_program: Vec<TranslationStatement>,
    pub translated_program: Vec<TranslationStatement>,
    /// The x64 line index of the instruction that last wrote each flag.
    /// Keyed by singleton `FlagSet` values (CF, PF, AF, ZF, SF, OF).
    last_flag_writer: HashMap<FlagSet, usize>,
    /// Maps each x64 line index to the union of flags that some later
    /// instruction reads from it.  Populated during pass 1 by `record_flags`;
    /// consumed by `flag_production_pass` in pass 2.
    flag_producers: HashMap<usize, FlagSet>,
}

impl Translator {
    pub fn new() -> Self {
        Self {
            reg_last_used: HashMap::new(),
            current_x86_idx: 0,
            src_program: Vec::new(),
            translated_program: Vec::new(),
            last_flag_writer: HashMap::new(),
            flag_producers: HashMap::new(),
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
            X64Opcode::Nop => self.translate_nop(instr),
            X64Opcode::Leave => self.translate_leave(instr),
            X64Opcode::Cmov(cond) => self.translate_cmov(instr, cond),
        }
    }

    fn translation_cleanup(&mut self) {
        self.translated_program.clear();
        self.reg_last_used.clear();
        self.current_x86_idx = 0;
        self.last_flag_writer.clear();
        self.flag_producers.clear();
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

        // Group ARM64 statement indices by their originating x64 index.
        // Built once here and shared with both pass-2 sub-passes.
        let x64_to_arm_idx_grouping = self.build_idx_groups();

        // Pass 2: resolve placeholders now that reg_last_used is complete,
        // then toggle flag-producing instructions based on flag_producer_indices.
        self.resolve_placeholders(&x64_to_arm_idx_grouping);
        self.flag_production_pass(&x64_to_arm_idx_grouping);

        // Prepend the cpu-info struct prologue so it runs before any
        // translated instruction.  Done last so it is invisible to both
        // pass-2 sub-passes.
        let prologue = self.emit_cpu_info_prologue();
        self.translated_program.splice(0..0, prologue);

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

    /// Updates `last_flag_writer` and `flag_producers` for one x64 instruction.
    ///
    /// Reads are processed before writes so that an instruction that both
    /// reads and writes a flag (e.g. a hypothetical ADC) finds the *previous*
    /// writer, not itself.
    fn record_flags(&mut self, instr: &Instruction) {
        let x64_idx = self.current_x86_idx;

        // 1. Reads — find who last wrote each flag we consume and record that
        //    we need it to produce the specific flag we're reading.
        for flag in instr.flags_read.iter() {
            if let Some(&writer_idx) = self.last_flag_writer.get(&flag) {
                *self
                    .flag_producers
                    .entry(writer_idx)
                    .or_insert(FlagSet::NONE) |= flag;
            }
        }

        // 2. Writes — this instruction becomes the new last writer.
        for flag in instr.flags_written.iter() {
            self.last_flag_writer.insert(flag, x64_idx);
        }
    }

    /// Pass 2 sub-pass: for every x64 group in `flag_producers`, split the
    /// needed flags into NZCV (toggle S-suffix) and emulated (insert parity
    /// computation sequence).
    fn flag_production_pass(&mut self, idx_groups: &HashMap<usize, Vec<usize>>) {
        // Snapshot so we can borrow self freely inside the loop.
        let producers: Vec<(usize, FlagSet)> =
            self.flag_producers.iter().map(|(&k, &v)| (k, v)).collect();

        let mut to_toggle: Vec<usize> = Vec::new();
        // (insert_position, instructions) sorted descending later
        let mut to_insert: Vec<(usize, Vec<TranslationStatement>)> = Vec::new();

        for (x64_idx, needed_flags) in producers {
            let Some(arm_indices) = idx_groups.get(&x64_idx) else {
                continue;
            };

            // ── NZCV flags: toggle S-suffix on the right ARM64 instruction ──
            let needed_nzcv = needed_flags & NZCV_FLAGS;
            if !needed_nzcv.is_empty() {
                // Does any instruction in this group inherently set all the
                // needed flags already (cmp / tst)?  If so, no toggle needed.
                let inherently_satisfied = arm_indices.iter().any(|&i| {
                    if let TranslationStatement::Instruction(instr, _) = &self.translated_program[i]
                    {
                        !(nzcv_flags_always_produced(instr) & needed_nzcv).is_empty()
                    } else {
                        false
                    }
                });

                if !inherently_satisfied {
                    // Find the LAST instruction that, when given its S-suffix,
                    // produces at least one of the needed NZCV flags.
                    // "Last" is correct: earlier instructions' flags would be
                    // overwritten before the branch reads them.
                    let candidate = arm_indices
                        .iter()
                        .copied()
                        .filter(|&i| {
                            if let TranslationStatement::Instruction(instr, _) =
                                &self.translated_program[i]
                            {
                                !(nzcv_flags_produced_if_toggled(instr) & needed_nzcv).is_empty()
                            } else {
                                false
                            }
                        })
                        .last();

                    match candidate {
                        Some(idx) => to_toggle.push(idx),
                        None => {
                            // TODO: no S-suffix variant in this group (e.g. mov,
                            // ldr).  Needs NZCV software emulation.
                        }
                    }
                }
            }

            // ── Emulated flags: insert parity sequence after the group ───────
            let needed_emulated = needed_flags & EMULATED_FLAGS;
            if needed_emulated.contains(FlagSet::PF) {
                // Re-derive the parity sequence from the original x64 instruction.
                let x64_instr = match self.src_program.get(x64_idx) {
                    Some(TranslationStatement::Instruction(instr, _)) => instr.clone(),
                    _ => continue,
                };
                let scratch = self.alloc_scratch();

                if let Some(parity_instrs) = emit_parity_for_x64_instr(&x64_instr, scratch) {
                    let insert_pos = arm_indices.last().copied().unwrap_or(0) + 1;
                    let stmts = parity_instrs
                        .into_iter()
                        .map(|i| TranslationStatement::Instruction(i, x64_idx))
                        .collect();
                    to_insert.push((insert_pos, stmts));
                }
            }
        }

        // Apply NZCV toggles.
        for arm_idx in to_toggle {
            if let TranslationStatement::Instruction(instr, _) =
                &mut self.translated_program[arm_idx]
            {
                instr.produces_flags = true;
            }
        }

        // Insert parity sequences back-to-front so earlier insertions
        // don't invalidate later positions.
        to_insert.sort_by(|a, b| b.0.cmp(&a.0));
        for (pos, stmts) in to_insert {
            let clamped = pos.min(self.translated_program.len());
            for (offset, stmt) in stmts.into_iter().enumerate() {
                self.translated_program.insert(clamped + offset, stmt);
            }
        }
    }

    /// Builds a map from x64 line index to the list of ARM64 statement indices
    /// that were translated from it.  Shared by both pass-2 sub-passes so the
    /// mapping is only constructed once.
    fn build_idx_groups(&self) -> HashMap<usize, Vec<usize>> {
        let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
        for (arm_idx, statement) in self.translated_program.iter().enumerate() {
            if let TranslationStatement::Instruction(_, x64_idx) = statement {
                groups.entry(*x64_idx).or_default().push(arm_idx);
            }
        }
        groups
    }

    /// Emits the cpu-info prologue: allocate the struct on the stack,
    /// point [`CPU_INFO_REG`] (x28) at it, and zero-initialise every field.
    ///
    /// Prepended to the translated program after both pass-2 sub-passes so
    /// placeholder resolution and flag toggling never see these instructions.
    /// The sentinel index `usize::MAX` marks them as synthetic.
    fn emit_cpu_info_prologue(&self) -> Vec<TranslationStatement> {
        let synthetic = usize::MAX;
        vec![
            // sub sp, sp, #CPU_INFO_SIZE
            TranslationStatement::Instruction(
                arm64_instr(
                    Arm64Opcode::Sub,
                    vec![
                        reg_operand(Arm64Reg::Sp, Width::W64, Role::Dest),
                        reg_operand(Arm64Reg::Sp, Width::W64, Role::Src),
                        imm_operand(CPU_INFO_SIZE, Width::W64, Role::Src),
                    ],
                ),
                synthetic,
            ),
            // add x28, sp, #0  (canonical way to copy SP to a GPR in ARM64)
            TranslationStatement::Instruction(
                arm64_instr(
                    Arm64Opcode::Add,
                    vec![
                        reg_operand(CPU_INFO_REG, Width::W64, Role::Dest),
                        reg_operand(Arm64Reg::Sp, Width::W64, Role::Src),
                        imm_operand(0, Width::W64, Role::Src),
                    ],
                ),
                synthetic,
            ),
            // str xzr, [x28]  — zero all 8 bytes (parity_flag + padding)
            TranslationStatement::Instruction(
                arm64_instr(
                    Arm64Opcode::Str,
                    vec![
                        mem_operand(
                            Arm64MemOperand {
                                base: CPU_INFO_REG,
                                offset: Some(0),
                                index: None,
                                modifier: Arm64Modifier::None,
                                pre_indexed: false,
                                post_indexed: false,
                            },
                            Width::W64,
                            Role::Dest,
                        ),
                        reg_operand(Arm64Reg::Xzr, Width::W64, Role::Src),
                    ],
                ),
                synthetic,
            ),
        ]
    }

    /// Returns the first ARM64 GPR candidate that has never appeared in
    /// the instruction stream so far (a "clean" register), or
    /// [`Arm64Reg::Placeholder`] carrying `current_x86_idx` if every
    /// candidate has been used at least once.
    pub(super) fn alloc_scratch(&self) -> Arm64Reg {
        // for &reg in SCRATCH_CANDIDATE_GPRS {
        //     if !self.reg_last_used.contains_key(&reg) {
        //         return reg;
        //     }
        // }
        // Arm64Reg::Placeholder(self.current_x86_idx as u32)
        //
        return SCRATCH_GPR;
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

/// Returns the NZCV-equivalent [`FlagSet`] that would be produced if this
/// ARM64 instruction is given its flag-setting S-suffix (`adds`, `subs`,
/// `eors`, `ands`).
///
/// The OR-based caller check `!(nzcv_produced_if_toggled(i) & needed).is_empty()`
/// means only instructions that produce **at least one** of the needed flags
/// are considered — hypothetical future opcodes that only set a subset of
/// NZCV are handled correctly without any AND-all-flags assumption.
///
/// Returns `FlagSet::NONE` for opcodes with no S-suffix variant.
fn nzcv_flags_produced_if_toggled(instr: &Instruction) -> FlagSet {
    match instr.opcode {
        // ADDS / SUBS / EORS / ANDS all set N, Z, C, V → SF, ZF, CF, OF.
        Opcode::Arm64(
            Arm64Opcode::Add | Arm64Opcode::Sub | Arm64Opcode::Eor | Arm64Opcode::And,
        ) => NZCV_FLAGS,
        _ => FlagSet::NONE,
    }
}

/// Returns the NZCV-equivalent flags that an ARM64 instruction sets
/// *inherently* (no toggle needed).
fn nzcv_flags_always_produced(instr: &Instruction) -> FlagSet {
    match instr.opcode {
        Opcode::Arm64(Arm64Opcode::Cmp | Arm64Opcode::Tst) => NZCV_FLAGS,
        _ => FlagSet::NONE,
    }
}
