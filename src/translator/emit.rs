//! Formats ARM64 `Instruction`s as GNU-assembler text and writes them to
//! a `.s` file.
//!
//! Scope: covers exactly the ARM64 opcodes/operand shapes that
//! `translate.rs` currently produces (`Mov`, `Add`, `Sub`, `Cmp`, `Eor`,
//! `Tst`, `Ldr`, `Str`, `Ret`). Branch opcodes (`B`, `BCond`, `Bl`) and
//! pair load/store (`Ldp`, `Stp`) return `EmitError::UnsupportedOpcode`
//! rather than a guess — `BCond` in particular can't be formatted
//! correctly yet since the enum doesn't carry a condition code, and
//! branch targets don't have a resolved-operand representation yet
//! either (same gap as the label-resolution pass discussed earlier).
//! No labels are emitted — `Line::Label` lowering is still `todo!()`
//! upstream, so there's nothing to attach them to yet.

use std::fs;
use std::io;
use std::path::Path;

use crate::translator::arm_modifiers::Arm64Modifier;
use crate::translator::arm_modifiers::ExtendKind;
use crate::translator::arm_modifiers::ShiftKind;
use crate::translator::instruction::Arch;
use crate::translator::instruction::Instruction;
use crate::translator::opcodes::Arm64Opcode;
use crate::translator::opcodes::Opcode;
use crate::translator::operand::Arm64MemOperand;
use crate::translator::operand::Arm64OperandKind;
use crate::translator::operand::Operand;
use crate::translator::operand::OperandKind;
use crate::translator::register::Arm64Reg;

#[derive(Debug, Clone, PartialEq)]
pub enum EmitError {
    /// `instruction.arch` says one thing but `instruction.opcode` says
    /// another — a malformed `Instruction`, not a translation gap.
    ArchOpcodeMismatch,
    NotArm64,
    UnsupportedOpcode(Arm64Opcode),
    UnsupportedOperand {
        opcode: Arm64Opcode,
        detail: &'static str,
    },
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmitError::ArchOpcodeMismatch => {
                write!(f, "instruction's arch and opcode fields disagree")
            }
            EmitError::NotArm64 => write!(f, "instruction is not ARM64"),
            EmitError::UnsupportedOpcode(op) => {
                write!(f, "text emission not implemented for {op:?} yet")
            }
            EmitError::UnsupportedOperand { opcode, detail } => write!(f, "{opcode:?}: {detail}"),
        }
    }
}

#[derive(Debug)]
pub enum WriteAsmError {
    Emit(EmitError),
    Io(io::Error),
}

impl From<EmitError> for WriteAsmError {
    fn from(e: EmitError) -> Self {
        WriteAsmError::Emit(e)
    }
}

impl From<io::Error> for WriteAsmError {
    fn from(e: io::Error) -> Self {
        WriteAsmError::Io(e)
    }
}

impl std::fmt::Display for WriteAsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteAsmError::Emit(e) => write!(f, "{e}"),
            WriteAsmError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Renders a full instruction list as GNU-assembler (Intel-style operand
/// order kept consistent with the rest of this codebase: dest before
/// src) ARM64 source text, one instruction per line, under a `.text`
/// section directive.
pub fn instructions_to_asm(instructions: &[Instruction]) -> Result<String, EmitError> {
    let mut out = String::new();
    out.push_str(".text\n");
    for instr in instructions {
        out.push_str("    ");
        out.push_str(&format_instruction(instr)?);
        out.push('\n');
    }
    Ok(out)
}

/// Renders and writes the instruction list to `path` as a `.s` file.
pub fn write_arm64_asm_file(
    instructions: &[Instruction],
    path: impl AsRef<Path>,
) -> Result<(), WriteAsmError> {
    let asm = instructions_to_asm(instructions)?;
    fs::write(path, asm)?;
    Ok(())
}

fn format_instruction(instr: &Instruction) -> Result<String, EmitError> {
    let op = match (instr.arch, instr.opcode) {
        (Arch::Arm64, Opcode::Arm64(op)) => op,
        (Arch::Arm64, Opcode::X64(_)) | (Arch::X64, Opcode::Arm64(_)) => {
            return Err(EmitError::ArchOpcodeMismatch);
        }
        (Arch::X64, Opcode::X64(_)) => return Err(EmitError::NotArm64),
    };

    let mnemonic = match op {
        Arm64Opcode::Mov => "mov",
        Arm64Opcode::Add => "add",
        Arm64Opcode::Sub => "sub",
        Arm64Opcode::Cmp => "cmp",
        Arm64Opcode::Eor => "eor",
        Arm64Opcode::Tst => "tst",
        Arm64Opcode::Ret => "ret",
        Arm64Opcode::Ldr => "ldr",
        Arm64Opcode::Str => "str",
        // Not produced by translate.rs yet, and not formattable correctly
        // yet even if they were (see module doc comment).
        Arm64Opcode::B
        | Arm64Opcode::BCond
        | Arm64Opcode::Bl
        | Arm64Opcode::Ldp
        | Arm64Opcode::Stp => return Err(EmitError::UnsupportedOpcode(op)),
    };

    if instr.operands.is_empty() {
        return Ok(mnemonic.to_string());
    }

    let operand_strs: Result<Vec<String>, EmitError> = instr
        .operands
        .iter()
        .map(|o| format_operand(op, o))
        .collect();
    let mut operand_strs = operand_strs?;

    // `str`'s internal operand order is [memory(Dest), register(Src)] to
    // keep Role meaningful for analysis (the memory location is what's
    // written). But ARM64 assembly syntax always writes str as
    // `str Xt, [mem]` — register first — regardless of role, unlike
    // `ldr` where role order and text order happen to coincide. Swap
    // only for the printed form; the `Instruction` itself is untouched.
    if matches!(op, Arm64Opcode::Str) {
        operand_strs.swap(0, 1);
    }

    Ok(format!("{mnemonic} {}", operand_strs.join(", ")))
}

fn format_operand(opcode: Arm64Opcode, operand: &Operand) -> Result<String, EmitError> {
    let OperandKind::Arm64(kind) = &operand.kind else {
        return Err(EmitError::UnsupportedOperand {
            opcode,
            detail: "expected an ARM64 operand kind",
        });
    };
    match kind {
        Arm64OperandKind::Register(reg, modifier) => Ok(format!(
            "{}{}",
            format_reg(*reg),
            format_modifier_suffix(*modifier)
        )),
        Arm64OperandKind::Immediate(n) => Ok(format!("#{n}")),
        Arm64OperandKind::Memory(mem) => Ok(format_mem(mem)),
        Arm64OperandKind::Condition(_) => Err(EmitError::UnsupportedOperand {
            opcode,
            detail: "condition operands not formattable yet",
        }),
    }
}

fn format_reg(reg: Arm64Reg) -> String {
    match reg {
        Arm64Reg::X(n) => format!("x{n}"),
        Arm64Reg::W(n) => format!("w{n}"),
        Arm64Reg::V(n) => format!("v{n}"),
        Arm64Reg::Sp => "sp".to_string(),
        Arm64Reg::Xzr => "xzr".to_string(),
        Arm64Reg::Placeholder(idx) => {
            panic!(
                "unresolved scratch-register placeholder (created at x86 index {idx}) reached the emitter — resolve_placeholders was not called or failed"
            )
        }
    }
}

fn format_modifier_suffix(modifier: Arm64Modifier) -> String {
    match modifier {
        Arm64Modifier::None => String::new(),
        Arm64Modifier::Shift(kind, amount) => {
            let name = match kind {
                ShiftKind::Lsl => "lsl",
                ShiftKind::Lsr => "lsr",
                ShiftKind::Asr => "asr",
                ShiftKind::Ror => "ror",
            };
            if amount == 0 {
                format!(", {name}")
            } else {
                format!(", {name} #{amount}")
            }
        }
        Arm64Modifier::Extend(kind, amount) => {
            let name = match kind {
                ExtendKind::Uxtb => "uxtb",
                ExtendKind::Uxth => "uxth",
                ExtendKind::Uxtw => "uxtw",
                ExtendKind::Uxtx => "uxtx",
                ExtendKind::Sxtb => "sxtb",
                ExtendKind::Sxth => "sxth",
                ExtendKind::Sxtw => "sxtw",
                ExtendKind::Sxtx => "sxtx",
            };
            if amount == 0 {
                format!(", {name}")
            } else {
                format!(", {name} #{amount}")
            }
        }
    }
}

fn format_mem(mem: &Arm64MemOperand) -> String {
    let base = format_reg(mem.base);

    if let Some(index) = mem.index {
        let idx = format_reg(index);
        let modifier = format_modifier_suffix(mem.modifier);
        return format!("[{base}, {idx}{modifier}]");
    }

    let offset = mem.offset.unwrap_or(0);
    if mem.pre_indexed {
        format!("[{base}, #{offset}]!")
    } else if mem.post_indexed {
        format!("[{base}], #{offset}")
    } else if offset == 0 {
        format!("[{base}]")
    } else {
        format!("[{base}, #{offset}]")
    }
}
