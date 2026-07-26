mod input;
mod translator;

use crate::input::parser::parse_asm;
use crate::translator::emit::write_arm64_asm_file;
use crate::translator::translator::Translator;
use std::env;
use std::fs;

fn main() {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "./tests/basic_no_labels.s".to_string());

    let src = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("failed to read {path}: {e}");
        std::process::exit(1);
    });

    let ast = parse_asm(&src).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        std::process::exit(1);
    });

    let mut translator = Translator::new();

    let loaded_x64 = translator.load_program(&ast).unwrap_or_else(|e| {
        eprintln!("load error: {e}");
        std::process::exit(1);
    });

    let arm64 = translator
        .translate_program(&loaded_x64)
        .unwrap_or_else(|e| {
            eprintln!("translation failed: {e}");
            std::process::exit(1);
        });

    write_arm64_asm_file(&arm64, "output.s").unwrap_or_else(|e| {
        eprintln!("failed to write output.s: {e}");
        std::process::exit(1);
    });
}
