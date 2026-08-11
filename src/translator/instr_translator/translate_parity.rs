//! ARM64 parity-flag emulation helpers.
//!
//! x86-64 sets PF based on the **parity of the low byte** of the result:
//! PF = 1 if the low byte has an even number of set bits; 0 otherwise.
//! ARM64 has no parity flag, so we compute it in software and store it in
//! the cpu-info struct (at [`offsets::PARITY_FLAG`], pointed to by x28).
//!
//! ## Parity computation (XOR-folding)
//!
//! ```text
//! and  scratch, result, #0xFF   ; isolate low byte
//! eor  scratch, scratch, scratch, lsr #4
//! eor  scratch, scratch, scratch, lsr #2
//! eor  scratch, scratch, scratch, lsr #1
//! and  scratch, scratch, #1     ; 1 = even parity (PF=1), 0 = odd (PF=0)
//! strb w<scratch>, [x28, #PF]   ; store into cpu-info struct
//! ```
//!
//! Two entry points are provided:
//!
//! * [`emit_parity_from_reg`] — for instructions whose result is **already
//!   in a register** (ADD, SUB, XOR, INC, DEC).  No recomputation needed.
//!
//! * [`emit_parity_computation`] — for instructions whose result is
//!   **discarded** by the ARM64 equivalent (CMP, TST).  Recomputes
//!   `lhs <op> rhs` into scratch first, then folds.
//!
//! Both are called lazily from `flag_production_pass` via
//! [`emit_parity_for_x64_instr`], which dispatches to the right variant
//! based on the original x64 opcode.

use crate::translator::arm_modifiers::ShiftKind;
use crate::translator::{
    arm_modifiers::Arm64Modifier,
    cpu_info::{CPU_INFO_REG, offsets},
    instruction::Instruction,
    opcodes::{Arm64Opcode, Opcode, X64Opcode},
    operand::{Arm64MemOperand, Operand, OperandKind, Role, X64OperandKind},
    register::Arm64Reg,
    util::{
        Width, arm64_instr, imm_operand, map_register_operand, mem_operand, reg_operand,
        shifted_reg_operand,
    },
};

// ── shared fold sequence ──────────────────────────────────────────────────────

/// Emits steps 2-7 of the parity sequence: isolate the low byte, XOR-fold
/// to a single parity bit, and store via STRB.
///
/// Assumes `scratch` already holds the result to measure.
fn fold_and_store(scratch: Arm64Reg, width: Width) -> Vec<Instruction> {
    let w_scratch = match scratch {
        Arm64Reg::X(n) => Arm64Reg::W(n),
        other => other, // Placeholder stays as-is; resolver will fix it
    };
    vec![
        arm64_instr(
            Arm64Opcode::And,
            vec![
                reg_operand(scratch, width, Role::Dest),
                reg_operand(scratch, width, Role::Src),
                imm_operand(0xFF, width, Role::Src),
            ],
        ),
        arm64_instr(
            Arm64Opcode::Eor,
            vec![
                reg_operand(scratch, width, Role::Dest),
                reg_operand(scratch, width, Role::Src),
                shifted_reg_operand(scratch, ShiftKind::Lsr, 4, width, Role::Src),
            ],
        ),
        arm64_instr(
            Arm64Opcode::Eor,
            vec![
                reg_operand(scratch, width, Role::Dest),
                reg_operand(scratch, width, Role::Src),
                shifted_reg_operand(scratch, ShiftKind::Lsr, 2, width, Role::Src),
            ],
        ),
        arm64_instr(
            Arm64Opcode::Eor,
            vec![
                reg_operand(scratch, width, Role::Dest),
                reg_operand(scratch, width, Role::Src),
                shifted_reg_operand(scratch, ShiftKind::Lsr, 1, width, Role::Src),
            ],
        ),
        arm64_instr(
            Arm64Opcode::And,
            vec![
                reg_operand(scratch, width, Role::Dest),
                reg_operand(scratch, width, Role::Src),
                imm_operand(1, width, Role::Src),
            ],
        ),
        arm64_instr(
            Arm64Opcode::Strb,
            vec![
                mem_operand(
                    Arm64MemOperand {
                        base: CPU_INFO_REG,
                        offset: Some(offsets::PARITY_FLAG),
                        index: None,
                        modifier: Arm64Modifier::None,
                        pre_indexed: false,
                        post_indexed: false,
                    },
                    Width::W8,
                    Role::Dest,
                ),
                reg_operand(w_scratch, Width::W32, Role::Src),
            ],
        ),
    ]
}

// ── public entry points ───────────────────────────────────────────────────────

/// Emits parity computation for instructions whose ARM64 result is already
/// in `result_reg` (ADD, SUB, XOR, INC, DEC).
///
/// `scratch` must differ from `result_reg`; it is overwritten.
pub fn emit_parity_from_reg(
    result_reg: Arm64Reg,
    width: Width,
    scratch: Arm64Reg,
) -> Vec<Instruction> {
    let mut instrs = vec![
        // Copy result into scratch so we don't destroy result_reg.
        arm64_instr(
            Arm64Opcode::And,
            vec![
                reg_operand(scratch, width, Role::Dest),
                reg_operand(result_reg, width, Role::Src),
                imm_operand(0xFF, width, Role::Src),
            ],
        ),
    ];
    // Replace the first AND in fold_and_store with the copy above, then
    // skip it by taking the tail (steps eor/eor/eor/and/strb).
    let mut tail = fold_and_store(scratch, width);
    tail.remove(0); // fold_and_store already starts with AND #0xFF; skip it
    instrs.extend(tail);
    instrs
}

/// Emits parity computation for instructions whose result is **discarded**
/// by the ARM64 equivalent (CMP → SUB, TST → AND).
///
/// Recomputes `lhs <op> rhs` into `scratch`, then folds.
pub fn emit_parity_computation(
    compute_op: Arm64Opcode,
    lhs: Arm64Reg,
    rhs: Operand,
    width: Width,
    scratch: Arm64Reg,
) -> Vec<Instruction> {
    let mut instrs = vec![arm64_instr(
        compute_op,
        vec![
            reg_operand(scratch, width, Role::Dest),
            reg_operand(lhs, width, Role::Src),
            rhs,
        ],
    )];
    instrs.extend(fold_and_store(scratch, width));
    instrs
}

/// Dispatches to the right parity helper based on the original x64 opcode.
///
/// Returns `None` if the instruction type is not yet handled (e.g. a memory
/// destination, or an opcode not in the model).
pub fn emit_parity_for_x64_instr(
    x64_instr: &Instruction,
    scratch: Arm64Reg,
) -> Option<Vec<Instruction>> {
    let x64_op = match x64_instr.opcode {
        Opcode::X64(op) => op,
        _ => return None,
    };

    match x64_op {
        // Result lives in the destination (first) operand.
        X64Opcode::Add | X64Opcode::Sub | X64Opcode::Xor | X64Opcode::Inc | X64Opcode::Dec => {
            let dest = x64_instr.operands.first()?;
            let (arm_reg, width) = x64_reg_to_arm(dest)?;
            Some(emit_parity_from_reg(arm_reg, width, scratch))
        }

        // CMP discards result in ARM64 cmp — recompute as subtraction.
        X64Opcode::Cmp => {
            let (lhs_reg, width) = x64_reg_to_arm(x64_instr.operands.first()?)?;
            let rhs = x64_operand_to_arm(x64_instr.operands.get(1)?, width)?;
            Some(emit_parity_computation(
                Arm64Opcode::Sub,
                lhs_reg,
                rhs,
                width,
                scratch,
            ))
        }

        // TST discards result in ARM64 tst — recompute as AND.
        X64Opcode::Test => {
            let (lhs_reg, width) = x64_reg_to_arm(x64_instr.operands.first()?)?;
            let rhs = x64_operand_to_arm(x64_instr.operands.get(1)?, width)?;
            Some(emit_parity_computation(
                Arm64Opcode::And,
                lhs_reg,
                rhs,
                width,
                scratch,
            ))
        }

        _ => None,
    }
}

// ── x64→ARM64 operand re-derivation ──────────────────────────────────────────

fn x64_reg_to_arm(op: &crate::translator::operand::Operand) -> Option<(Arm64Reg, Width)> {
    match &op.kind {
        OperandKind::X64(X64OperandKind::Register(r)) => map_register_operand(*r).ok(),
        _ => None,
    }
}

fn x64_operand_to_arm(op: &crate::translator::operand::Operand, width: Width) -> Option<Operand> {
    match &op.kind {
        OperandKind::X64(X64OperandKind::Register(r)) => {
            let (arm_reg, _) = map_register_operand(*r).ok()?;
            Some(reg_operand(arm_reg, width, Role::Src))
        }
        OperandKind::X64(X64OperandKind::Immediate(n)) => Some(imm_operand(*n, width, Role::Src)),
        // Memory operands would need a load-first pass — TODO
        _ => None,
    }
}
