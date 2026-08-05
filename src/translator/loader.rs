use crate::{
    input::ast::{Line, ParsedInstruction, ParsedMem, ParsedOperand, Size},
    translator::{
        instruction::{Arch, Instruction},
        opcodes::{Opcode, X64Condition, X64Opcode},
        operand::{
            Operand, OperandKind,
            Role::{self, Dest, Src, SrcDest},
            SegmentReg, X64AddrBase, X64MemOperand, X64OperandKind,
        },
        register::{X64GpReg, resolve_segment_register, resolve_x64_register},
        statement::{Label, TranslationStatement},
        util::Width,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub enum LoaderError {
    UnknownMnemonic {
        mnemonic: String,
        line: usize,
    },
    UnknownRegister {
        name: String,
        line: usize,
    },
    InvalidAddressRegister {
        name: String,
        line: usize,
    },
    OperandCountMismatch {
        mnemonic: String,
        expected: usize,
        found: usize,
        line: usize,
    },
    MismatchedOperandSizes {
        mnemonic: String,
        a: Width,
        b: Width,
        line: usize,
    },
    AmbiguousMemorySize {
        line: usize,
    },
    DisplacementOutOfRange {
        value: i64,
        line: usize,
    },
    /// Jump/call targets aren't resolved here — see the module doc comment.
    UnresolvedLabel {
        name: String,
        line: usize,
    },
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::UnknownMnemonic { mnemonic, line } => {
                write!(f, "line {line}: unknown mnemonic '{mnemonic}'")
            }
            LoaderError::UnknownRegister { name, line } => {
                write!(f, "line {line}: unknown register '{name}'")
            }
            LoaderError::InvalidAddressRegister { name, line } => {
                write!(
                    f,
                    "line {line}: '{name}' can't be used as an address base/index register"
                )
            }
            LoaderError::OperandCountMismatch {
                mnemonic,
                expected,
                found,
                line,
            } => {
                write!(
                    f,
                    "line {line}: '{mnemonic}' expects {expected} operand(s), found {found}"
                )
            }
            LoaderError::MismatchedOperandSizes {
                mnemonic,
                a,
                b,
                line,
            } => {
                write!(
                    f,
                    "line {line}: '{mnemonic}' has operands of conflicting sizes ({a:?} vs {b:?})"
                )
            }
            LoaderError::AmbiguousMemorySize { line } => {
                write!(
                    f,
                    "line {line}: memory operand size is ambiguous — add a size prefix (e.g. 'dword ptr')"
                )
            }
            LoaderError::DisplacementOutOfRange { value, line } => {
                write!(
                    f,
                    "line {line}: displacement {value} doesn't fit in x64's 32-bit signed range"
                )
            }
            LoaderError::UnresolvedLabel { name, line } => {
                write!(
                    f,
                    "line {line}: label '{name}' not resolved (requires a symbol-table pass over the whole program)"
                )
            }
        }
    }
}

/// Lowers every line of a parsed program into a [`TranslatedStatement`],
/// stopping at the first error.
pub fn load_program(lines: &[Line]) -> Result<Vec<TranslationStatement>, LoaderError> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| lower_line(line, index))
        .collect()
}

fn lower_line(line: &Line, line_index: usize) -> Result<TranslationStatement, LoaderError> {
    match line {
        Line::Instruction(pi) => {
            lower_instruction(pi).map(|instr| TranslationStatement::Instruction(instr, line_index))
        }
        Line::Label(name) => Ok(TranslationStatement::Label(Label { name: name.clone() })),
        Line::Directive(d) => Ok(TranslationStatement::Directive(
            crate::translator::statement::Directive {
                name: d.name.clone(),
                args: d.args.clone(),
                line: d.line,
            },
        )),
    }
}

fn lower_instruction(pi: &ParsedInstruction) -> Result<Instruction, LoaderError> {
    let mnemonic = pi.mnemonic.to_ascii_lowercase();
    let (opcode, roles) = opcode_and_roles(&mnemonic, pi.line)?;

    if roles.len() != pi.operands.len() {
        return Err(LoaderError::OperandCountMismatch {
            mnemonic,
            expected: roles.len(),
            found: pi.operands.len(),
            line: pi.line,
        });
    }

    let known_width = infer_operand_width(pi)?;
    let is_lea = matches!(opcode, Opcode::X64(X64Opcode::Lea));

    let mut operands = Vec::with_capacity(pi.operands.len());
    for (parsed, role) in pi.operands.iter().zip(roles.iter().copied()) {
        operands.push(lower_operand(parsed, role, known_width, is_lea, pi.line)?);
    }

    Ok(Instruction {
        arch: Arch::X64,
        opcode,
        operands,
        address: 0, // filled in once a symbol-table/layout pass assigns real addresses
        length: 0,  // filled in once you have an encoder
    })
}

/// Every register operand (and every explicit memory-size prefix) pins
/// down a width; if two disagree, that's a real assembly error (`mov
/// eax, [rbx+1]` with a `qword`-sized memory prefix would be nonsense) —
/// catching it here is cheap and worth doing.
fn infer_operand_width(pi: &ParsedInstruction) -> Result<Option<Width>, LoaderError> {
    let mut known: Option<Width> = None;
    for op in &pi.operands {
        let w = match op {
            ParsedOperand::Register(name) => Some(
                resolve_x64_register(name)
                    .ok_or_else(|| LoaderError::UnknownRegister {
                        name: name.clone(),
                        line: pi.line,
                    })?
                    .width(),
            ),
            ParsedOperand::Memory(m) => m.size.as_ref().map(size_to_width),
            ParsedOperand::Immediate(_) | ParsedOperand::LabelRef(_) => None,
        };

        if let Some(w) = w {
            match known {
                None => known = Some(w),
                Some(existing) if existing == w => {}
                Some(existing) => {
                    return Err(LoaderError::MismatchedOperandSizes {
                        mnemonic: pi.mnemonic.clone(),
                        a: existing,
                        b: w,
                        line: pi.line,
                    });
                }
            }
        }
    }
    Ok(known)
}

fn lower_operand(
    parsed: &ParsedOperand,
    role: Role,
    target_width: Option<Width>,
    is_lea: bool,
    line: usize,
) -> Result<Operand, LoaderError> {
    match parsed {
        ParsedOperand::Register(name) => {
            let reg = resolve_x64_register(name).ok_or_else(|| LoaderError::UnknownRegister {
                name: name.clone(),
                line,
            })?;
            Ok(Operand {
                kind: OperandKind::X64(X64OperandKind::Register(reg)),
                width: reg.width(),
                role,
            })
        }
        ParsedOperand::Immediate(n) => {
            // x64 immediates don't carry their own width in the syntax —
            // they take on the width of the operand they're paired with.
            // Defaulting to W32 when nothing else pins it down (e.g. an
            // instruction with only an immediate operand) is a reasonable
            // fallback but worth revisiting once you have real encodings
            // to check against (8-bit sign-extended immediate forms exist
            // for several of these opcodes and are smaller to encode).
            let width = target_width.unwrap_or(Width::W32);
            Ok(Operand {
                kind: OperandKind::X64(X64OperandKind::Immediate(*n)),
                width,
                role,
            })
        }
        ParsedOperand::Memory(m) => {
            let width = if is_lea {
                // `lea` computes an address, it doesn't dereference one —
                // its memory "operand" never actually accesses memory, so
                // there's no size to infer from a load/store width. The
                // result width comes from the destination register.
                target_width.unwrap_or(Width::W64)
            } else {
                m.size
                    .as_ref()
                    .map(size_to_width)
                    .or(target_width)
                    .ok_or(LoaderError::AmbiguousMemorySize { line })?
            };
            let mem = lower_mem_operand(m, line)?;
            Ok(Operand {
                kind: OperandKind::X64(X64OperandKind::Memory(mem)),
                width,
                role,
            })
        }
        ParsedOperand::LabelRef(name) => Ok(Operand {
            kind: OperandKind::X64(X64OperandKind::Label(name.clone())),
            width: Width::W64,
            role,
        }),
    }
}

fn lower_mem_operand(m: &ParsedMem, line: usize) -> Result<X64MemOperand, LoaderError> {
    let base = match &m.base {
        None => None,
        Some(name) if name.eq_ignore_ascii_case("rip") => Some(X64AddrBase::Rip),
        Some(name) => Some(X64AddrBase::Reg(resolve_addr_gpr(name, line)?)),
    };
    let index = match &m.index {
        None => None,
        Some(name) => Some(resolve_addr_gpr(name, line)?),
    };
    let segment: Option<SegmentReg> =
        match &m.segment {
            None => None,
            Some(name) => Some(resolve_segment_register(name).ok_or_else(|| {
                LoaderError::UnknownRegister {
                    name: name.clone(),
                    line,
                }
            })?),
        };
    let disp = i32::try_from(m.disp).map_err(|_| LoaderError::DisplacementOutOfRange {
        value: m.disp,
        line,
    })?;

    Ok(X64MemOperand {
        base,
        index,
        scale: m.scale.unwrap_or(1),
        disp,
        segment,
    })
}

/// Resolves a register name used as an address base/index. Addressing
/// always uses the full physical register regardless of the width being
/// loaded/stored (`[eax]` naming a 32-bit register would be a rare
/// address-size override, not a normal case) — so this rejects anything
/// that isn't a plain GPR, e.g. an xmm register can't be a base.
fn resolve_addr_gpr(name: &str, line: usize) -> Result<X64GpReg, LoaderError> {
    let reg = resolve_x64_register(name).ok_or_else(|| LoaderError::UnknownRegister {
        name: name.to_string(),
        line,
    })?;
    reg.parent_gpr()
        .ok_or_else(|| LoaderError::InvalidAddressRegister {
            name: name.to_string(),
            line,
        })
}

fn size_to_width(size: &Size) -> Width {
    match size {
        Size::Byte => Width::W8,
        Size::Word => Width::W16,
        Size::Dword => Width::W32,
        Size::Qword => Width::W64,
        Size::Xmmword => Width::W128,
        Size::Ymmword => Width::W256,
    }
}

/// Mnemonic -> (opcode, expected operand roles in order). This is the
/// per-opcode semantics table flagged as needed all the way back when
/// `Role` was first introduced — this is where it actually lives.
///
/// Deliberately incomplete: covers what's needed to lower `sample.s`.
/// Extending it is mechanical — add a match arm with the opcode and its
/// operand roles.
fn opcode_and_roles(mnemonic: &str, line: usize) -> Result<(Opcode, &'static [Role]), LoaderError> {
    match mnemonic {
        "mov" => Ok((Opcode::X64(X64Opcode::Mov), &[Dest, Src])),
        "lea" => Ok((Opcode::X64(X64Opcode::Lea), &[Dest, Src])),
        "add" => Ok((Opcode::X64(X64Opcode::Add), &[SrcDest, Src])),
        "sub" => Ok((Opcode::X64(X64Opcode::Sub), &[SrcDest, Src])),
        "xor" => Ok((Opcode::X64(X64Opcode::Xor), &[SrcDest, Src])),
        "cmp" => Ok((Opcode::X64(X64Opcode::Cmp), &[Src, Src])),
        "test" => Ok((Opcode::X64(X64Opcode::Test), &[Src, Src])),
        "inc" => Ok((Opcode::X64(X64Opcode::Inc), &[SrcDest])),
        "dec" => Ok((Opcode::X64(X64Opcode::Dec), &[SrcDest])),
        "push" => Ok((Opcode::X64(X64Opcode::Push), &[Src])),
        "pop" => Ok((Opcode::X64(X64Opcode::Pop), &[Dest])),
        "call" => Ok((Opcode::X64(X64Opcode::Call), &[Src])),
        "ret" => Ok((Opcode::X64(X64Opcode::Ret), &[])),
        "jmp" => Ok((Opcode::X64(X64Opcode::Jmp), &[Src])),
        // `mul`'s implicit rdx:rax destination isn't modeled as an operand
        // here yet — see the "implicit operands should be explicit" note
        // from the original design. Flagging rather than silently ignoring.
        "mul" => Ok((Opcode::X64(X64Opcode::Mul), &[Src])),
        other => {
            if let Some(cond_str) = other.strip_prefix('j') {
                if let Some(cond) = resolve_x64_condition(cond_str) {
                    return Ok((Opcode::X64(X64Opcode::Jcc(cond)), &[Src]));
                }
            }
            Err(LoaderError::UnknownMnemonic {
                mnemonic: mnemonic.to_string(),
                line,
            })
        }
    }
}

/// Maps a `jcc` mnemonic's suffix (e.g. `"ge"` from `jge`) to its
/// condition, including the common aliases (`jz`==`je`, `jnb`==`jae`, ...).
fn resolve_x64_condition(suffix: &str) -> Option<X64Condition> {
    use X64Condition::*;

    match suffix {
        "e" | "z" => Some(E),
        "ne" | "nz" => Some(Ne),
        "g" | "nle" => Some(G),
        "ge" | "nl" => Some(Ge),
        "l" | "nge" => Some(L),
        "le" | "ng" => Some(Le),
        "a" | "nbe" => Some(A),
        "ae" | "nb" | "nc" => Some(Ae),
        "b" | "nae" | "c" => Some(B),
        "be" | "na" => Some(Be),
        "s" => Some(S),
        "ns" => Some(Ns),
        "o" => Some(O),
        "no" => Some(No),
        "p" | "pe" => Some(P),
        "np" | "po" => Some(Np),
        _ => None,
    }
}
