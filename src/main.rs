mod input;
mod translator;

use crate::input::parser::parse_asm;
use crate::translator::emit::instructions_to_asm;
use crate::translator::emit::write_arm64_asm_file;
use std::env;
use std::fs;

fn main() {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "./tests/basic_sample.s".to_string());

    let src = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("failed to read {path}: {e}");
        std::process::exit(1);
    });

    let ast = parse_asm(&src).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        std::process::exit(1);
    });

    let loaded_x64 = crate::translator::loader::load_program(&ast).unwrap_or_else(|e| {
        eprintln!("load error: {e}");
        std::process::exit(1);
    });

    let arm64_result = loaded_x64.iter().map(|instruction| instruction.to_arm64()).collect::<Vec<_>>();
    let is_translation_error = arm64_result.iter().any(|result| result.is_err());

    if is_translation_error {
        eprintln!("translation failed");
        std::process::exit(1);
    }

    let arm64 = arm64_result.into_iter().flat_map(|result| result.unwrap()).collect::<Vec<_>>();
    write_arm64_asm_file(&arm64, "output.s").unwrap_or_else(|e| {
        eprintln!("failed to write output.s: {e}");
        std::process::exit(1);
    });
}
