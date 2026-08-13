//! Post-translation immediate-range legalization pass.
//!
//! ARM64 has stricter immediate encoding constraints than x86-64:
//!
//!   * `ADD`/`SUB` immediate: 0–4095 (12-bit unsigned), optionally shifted
//!     left by 12.  Negative values are not allowed; use the opposite opcode.
//!
//!   * `LDR`/`STR` immediate offset: −256..=255 (unscaled signed, 9-bit) or
//!     0..(4095 × access-size) with alignment (unsigned scaled, 12-bit).
//!
//! Instructions that fall outside these ranges are rewritten into legal
//! multi-instruction sequences using x24 as a temporary.  When x24 is
//! already the destination register (e.g. `add x24, x29, #-8048`) the
//! two-instruction form still works correctly because ARM64 reads all source
//! registers before writing the destination:
//!
//! ```asm
//!   mov x24, #8048          // x24 = 8048
//!   sub x24, x29, x24       // x24 = x29 - 8048  (reads old x24 on RHS)
//! ```
//!
//! x25 is used as the fallback scratch when x24 is already occupied as a
//! source operand in the same instruction.

use crate::translator::{
    arm_modifiers::Arm64Modifier, instruction::{Arch, Instruction}, opcodes::{Arm64Opcode, Opcode}, operand::{Arm64MemOperand, Arm64OperandKind, Operand, OperandKind, Role}, register::Arm64Reg, statement::TranslationStatement, translator::SCRATCH_GPR, util::{Width, arm64_instr, imm_operand, mem_operand, reg_operand},
};

/// Primary scratch register for legalization sequences (= SCRATCH_GPR).
const LEG_SCRATCH: Arm64Reg = SCRATCH_GPR;
/// Fallback scratch when x24 is a source operand in the same instruction.
const LEG_SCRATCH2: Arm64Reg = Arm64Reg::X(25);

// ── Constraint helpers ────────────────────────────────────────────────────────

/// Returns `true` if `imm` can be encoded as an ARM64 12-bit unsigned
/// immediate, optionally left-shifted by 12 bits.
fn is_valid_imm12(imm: i64) -> bool {
    if imm < 0 {
        return false;
    }
    (imm <= 4095) || (imm % 4096 == 0 && (imm >> 12) <= 4095)
}

/// Returns `true` if `offset` is encodable in a `LDR`/`STR` instruction for
/// an access of `size_bytes` bytes.
///
/// Two valid forms:
/// * Unscaled signed (`LDUR`/`STUR`): −256..=255 for any size.
/// * Unsigned scaled (`LDR`/`STR`): 0 to (4095 × size), aligned to size.
fn is_valid_mem_offset(offset: i32, size_bytes: u32) -> bool {
    if offset >= -256 && offset <= 255 {
        return true; // unscaled signed form
    }
    if offset < 0 {
        return false; // negative and out of unscaled range
    }
    let max_scaled: i32 = 4095 * size_bytes as i32;
    offset <= max_scaled && offset % size_bytes as i32 == 0
}

fn size_bytes_from_width(width: Width) -> u32 {
    match width {
        Width::W8 => 1,
        Width::W16 => 2,
        Width::W32 => 4,
        Width::W64 | Width::W128 | Width::W256 | Width::W512 => 8,
    }
}

// ── Scratch selection ─────────────────────────────────────────────────────────

/// Returns `LEG_SCRATCH` (x24) unless it appears in `exclude`, in which case
/// `LEG_SCRATCH2` (x25) is returned instead.
fn pick_scratch(exclude: &[Arm64Reg]) -> Arm64Reg {
    if exclude.contains(&LEG_SCRATCH) {
        LEG_SCRATCH2
    } else {
        LEG_SCRATCH
    }
}

// ── Address materialization ───────────────────────────────────────────────────

/// Emits a 1- or 2-instruction sequence that loads `base + offset` into
/// `scratch`.
///
/// * If `|offset|` fits as an imm12: `add/sub scratch, base, #|offset|`
/// * Otherwise: `mov scratch, #|offset|` then `add/sub scratch, base, scratch`
///   (the read of `scratch` on the right-hand side happens before the write)
fn materialize_address(scratch: Arm64Reg, base: Arm64Reg, offset: i32) -> Vec<Instruction> {
    let abs_offset = offset.unsigned_abs() as i64;
    let arm_op = if offset >= 0 {
        Arm64Opcode::Add
    } else {
        Arm64Opcode::Sub
    };

    if is_valid_imm12(abs_offset) {
        return vec![arm64_instr(
            arm_op,
            vec![
                reg_operand(scratch, Width::W64, Role::Dest),
                reg_operand(base, Width::W64, Role::Src),
                imm_operand(abs_offset, Width::W64, Role::Src),
            ],
        )];
    }

    // abs_offset > 4095: two-instruction sequence.
    vec![
        arm64_instr(
            Arm64Opcode::Mov,
            vec![
                reg_operand(scratch, Width::W64, Role::Dest),
                imm_operand(abs_offset, Width::W64, Role::Src),
            ],
        ),
        arm64_instr(
            arm_op,
            vec![
                reg_operand(scratch, Width::W64, Role::Dest),
                reg_operand(base, Width::W64, Role::Src),
                reg_operand(scratch, Width::W64, Role::Src),
            ],
        ),
    ]
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Walks `program` and replaces any ARM64 instruction that contains an
/// out-of-range immediate or memory offset with a legal multi-instruction
/// sequence.  All other statements pass through unchanged.
pub fn legalize_immediates(program: Vec<TranslationStatement>) -> Vec<TranslationStatement> {
    let mut result = Vec::with_capacity(program.len() + 32);

    for stmt in program {
        match &stmt {
            TranslationStatement::Instruction(instr, x64_idx) => {
                let replacements = legalize_instruction(instr);
                if replacements.is_empty() {
                    result.push(stmt);
                } else {
                    let idx = *x64_idx;
                    for new_instr in replacements {
                        result.push(TranslationStatement::Instruction(new_instr, idx));
                    }
                }
            }
            _ => result.push(stmt),
        }
    }

    result
}

// ── Per-instruction legalization ──────────────────────────────────────────────

/// Returns a replacement sequence for `instr`, or an empty `Vec` when the
/// instruction is already legal.
fn legalize_instruction(instr: &Instruction) -> Vec<Instruction> {
    let op = match (instr.arch, instr.opcode) {
        (Arch::Arm64, Opcode::Arm64(op)) => op,
        _ => return vec![],
    };

    match op {
        Arm64Opcode::Add | Arm64Opcode::Sub => {
            try_legalize_add_sub(instr, op).unwrap_or_default()
        }
        Arm64Opcode::Ldr | Arm64Opcode::Ldrb | Arm64Opcode::Str | Arm64Opcode::Strb => {
            try_legalize_mem(instr).unwrap_or_default()
        }
        _ => vec![],
    }
}

// ── ADD / SUB legalization ────────────────────────────────────────────────────

fn try_legalize_add_sub(instr: &Instruction, op: Arm64Opcode) -> Option<Vec<Instruction>> {
    // Only the 3-operand `add/sub dst, src, #imm` form is handled.
    if instr.operands.len() != 3 {
        return None;
    }
    let imm = match &instr.operands[2].kind {
        OperandKind::Arm64(Arm64OperandKind::Immediate(n)) => *n,
        _ => return None,
    };

    // Compute the effective (unsigned) value and the corrected opcode.
    // A negative immediate flips ADD ↔ SUB.
    let (real_op, abs_imm) = if imm < 0 {
        let flipped = if op == Arm64Opcode::Add {
            Arm64Opcode::Sub
        } else {
            Arm64Opcode::Add
        };
        (flipped, (-imm) as u64)
    } else {
        (op, imm as u64)
    };

    // Already legal and correctly signed — no change.
    if imm >= 0 && is_valid_imm12(imm) {
        return None;
    }

    let dst = &instr.operands[0];
    let src = &instr.operands[1];
    let width = dst.width;
    let dst_reg = reg_from_operand(dst)?;
    let src_reg = reg_from_operand(src)?;

    // Case 1: sign-flipped but fits as imm12 — emit a single flipped instruction.
    if is_valid_imm12(abs_imm as i64) {
        let mut new_instr = arm64_instr(
            real_op,
            vec![
                reg_operand(dst_reg, width, Role::Dest),
                reg_operand(src_reg, width, Role::Src),
                imm_operand(abs_imm as i64, width, Role::Src),
            ],
        );
        new_instr.produces_flags = instr.produces_flags;
        return Some(vec![new_instr]);
    }

    // Case 2: value doesn't fit as imm12 — materialize in a scratch register.
    //
    // Use LEG_SCRATCH2 only when LEG_SCRATCH appears as a *source* operand
    // *and* is not the destination.  When dst == LEG_SCRATCH the two-step
    // sequence `mov x24, #N; real_op x24, src, x24` is safe because ARM64
    // reads all sources before committing the result.
    let scratch = if src_reg == LEG_SCRATCH && dst_reg != LEG_SCRATCH {
        LEG_SCRATCH2
    } else {
        LEG_SCRATCH
    };

    let mov_instr = arm64_instr(
        Arm64Opcode::Mov,
        vec![
            reg_operand(scratch, Width::W64, Role::Dest),
            imm_operand(abs_imm as i64, Width::W64, Role::Src),
        ],
    );
    let mut op_instr = arm64_instr(
        real_op,
        vec![
            reg_operand(dst_reg, width, Role::Dest),
            reg_operand(src_reg, width, Role::Src),
            reg_operand(scratch, width, Role::Src),
        ],
    );
    op_instr.produces_flags = instr.produces_flags;

    Some(vec![mov_instr, op_instr])
}

// ── LDR / LDRB / STR / STRB legalization ─────────────────────────────────────

fn try_legalize_mem(instr: &Instruction) -> Option<Vec<Instruction>> {
    // Find the memory operand.
    let mem_idx = instr.operands.iter().position(|o| {
        matches!(&o.kind, OperandKind::Arm64(Arm64OperandKind::Memory(_)))
    })?;

    let mem_op = match &instr.operands[mem_idx].kind {
        OperandKind::Arm64(Arm64OperandKind::Memory(m)) => m.clone(),
        _ => return None,
    };

    // Only fix base + scalar offset; indexed forms ([base, xN, lsl #k]) are
    // always legal and don't need modification.
    let offset = match (mem_op.index, mem_op.offset) {
        (None, Some(off)) => off,
        (None, None) => return None, // [base] is always valid
        _ => return None,            // indexed — no fixup needed
    };

    let size_bytes = size_bytes_from_width(instr.operands[mem_idx].width);
    if is_valid_mem_offset(offset, size_bytes) {
        return None;
    }

    // Collect registers that must not be used as the scratch address register:
    // the base register and the data register.
    let data_reg = instr.operands.iter().find_map(|o| match &o.kind {
        OperandKind::Arm64(Arm64OperandKind::Register(r, _)) => Some(*r),
        _ => None,
    });
    let mut exclude = vec![mem_op.base];
    if let Some(r) = data_reg {
        exclude.push(r);
    }
    let scratch = pick_scratch(&exclude);

    // Emit address-computation prefix.
    let mut new_instrs = materialize_address(scratch, mem_op.base, offset);

    // Replace the memory operand with `[scratch]` (zero offset).
    let new_mem = Arm64MemOperand {
        base: scratch,
        offset: None,
        index: None,
        modifier: Arm64Modifier::None,
        pre_indexed: false,
        post_indexed: false,
    };
    let mem_width = instr.operands[mem_idx].width;
    let mem_role = instr.operands[mem_idx].role;

    let mut new_operands = instr.operands.clone();
    new_operands[mem_idx] = mem_operand(new_mem, mem_width, mem_role);

    let mut new_instr = instr.clone();
    new_instr.operands = new_operands;
    new_instrs.push(new_instr);

    Some(new_instrs)
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Extracts the `Arm64Reg` from a register operand, or returns `None`.
fn reg_from_operand(op: &Operand) -> Option<Arm64Reg> {
    match &op.kind {
        OperandKind::Arm64(Arm64OperandKind::Register(r, _)) => Some(*r),
        _ => None,
    }
}
