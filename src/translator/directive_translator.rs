//! Translates GAS directives from x86-64 form to their ARM64 equivalents.
//!
//! Most directives are assembler-generic and pass through unchanged.  The
//! cases that need transformation fall into four groups:
//!
//! | Group | Example | Action |
//! |---|---|---|
//! | x86-only syntax | `.intel_syntax`, `.code64` | **Drop** |
//! | Word-size rename | `.word` (2 B on x86) | **→ `.hword`** (ARM `.word` is 4 B) |
//! | Alignment | `.align N` (N bytes on x86) | **→ `.balign N`** (ARM `.align N` means 2^N) |
//! | Symbol type | `.type f, @function` | **→ `.type f, %function`** (`@` is a comment on ARM GAS) |

use crate::input::ast::DirectiveArg;
use crate::translator::statement::Directive;

/// Transforms one x86-64 GAS directive into its ARM64 GAS equivalent.
///
/// Returns `None` for directives that should be dropped entirely (they are
/// x86-specific with no ARM64 equivalent and would cause an assembler error
/// if left in the output).
pub fn translate_directive(dir: &Directive) -> Option<Directive> {
    let name = dir.name.as_str();

    match name {
        // ── x86-only: drop ──────────────────────────────────────────────
        // Syntax-mode selectors and operand-size overrides are meaningless
        // to an ARM64 assembler and must not appear in the output.
        "intel_syntax" | "att_syntax" | "code16" | "code32" | "code64" => None,

        // ── Sections: pass through ───────────────────────────────────────
        "text" | "data" | "bss" | "section" | "pushsection" | "popsection" | "previous" => {
            passthrough(dir)
        }

        // ── Symbol visibility & binding: pass through ────────────────────
        "global" | "globl" | "local" | "weak" | "hidden" | "protected" | "internal" | "extern"
        | "comm" | "lcomm" => passthrough(dir),

        // ── .type: remap @function/@object notation to %function/%object ─
        // On ARM GAS `@` starts a line comment, so STT type names must use
        // the `%` sigil instead.
        "type" => Some(Directive {
            name: "type".to_string(),
            args: dir.args.iter().map(remap_type_arg).collect(),
            line: dir.line,
        }),

        // ── .size: pass through ──────────────────────────────────────────
        "size" => passthrough(dir),

        // ── Data directives ──────────────────────────────────────────────
        // `.byte` (1 B) and `.quad`/`.octa` (8 B / 16 B) are identical on
        // both platforms and pass through.
        "byte" | "quad" | "8byte" | "octa" | "ascii" | "asciz" | "string" | "zero" | "space"
        | "fill" => passthrough(dir),

        // x86 `.word` / `.short` / `.2byte` = 2 bytes.
        // ARM GAS `.word` = 4 bytes, so we must rename to `.hword` (always 2 B).
        "word" | "short" | "2byte" => rename(dir, "hword"),

        // x86 `.long` / `.int` / `.4byte` = 4 bytes.
        // ARM GAS `.word` = 4 bytes, which matches.
        "long" | "int" | "4byte" => rename(dir, "word"),

        // ── Alignment ───────────────────────────────────────────────────
        // On x86 GAS `.align N` means "align to an N-byte boundary".
        // On ARM GAS `.align N` means "align to a 2^N-byte boundary".
        // `.balign N` means "align to an N-byte boundary" on *all* GAS
        // targets, so we always translate to that form.
        "align" => rename(dir, "balign"),
        // `.p2align N` (power-of-two) and `.balign N` are already
        // platform-neutral; leave them alone.
        "p2align" | "balign" => passthrough(dir),

        // ── CFI (Call Frame Information): pass through ───────────────────
        // CFI directives encode unwind info and are assembler-generic.
        // The register numbers may need adjustment if/when CFI is fully
        // supported, but the directive names themselves are unchanged.
        name if name.starts_with("cfi_") => passthrough(dir),

        // ── Miscellaneous: pass through ──────────────────────────────────
        "file" | "ident" | "equ" | "set" | "include" | "if" | "ifdef" | "ifndef" | "else"
        | "elseif" | "endif" | "rept" | "endr" | "irp" | "irpc" | "macro" | "endm"
        | "noaltmacro" | "altmacro" => passthrough(dir),

        // Unknown directive — pass through so we don't silently swallow
        // anything the user wrote; the ARM64 assembler will reject it if
        // it truly has no meaning there.
        _ => passthrough(dir),
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn passthrough(dir: &Directive) -> Option<Directive> {
    Some(dir.clone())
}

fn rename(dir: &Directive, new_name: &str) -> Option<Directive> {
    Some(Directive {
        name: new_name.to_string(),
        args: dir.args.clone(),
        line: dir.line,
    })
}

/// Replaces an `@`-prefixed type name with the ARM64 `%`-prefixed form.
///
/// GAS on ARM uses `%function`, `%object`, etc. because `@` is treated as a
/// line comment character.  On x86 GAS the same names are written with `@`.
/// Example: `DirectiveArg::Ident("@function")` → `DirectiveArg::Ident("%function")`.
fn remap_type_arg(arg: &DirectiveArg) -> DirectiveArg {
    match arg {
        DirectiveArg::Ident(s) if s.starts_with('@') => {
            DirectiveArg::Ident(format!("%{}", &s[1..]))
        }
        other => other.clone(),
    }
}
