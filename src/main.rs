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
        .unwrap_or_else(|| "./tests/basic_sample.s".to_string());

    let src = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("failed to read {path}: {e}");
        std::process::exit(1);
    });

    let ast = parse_asm(&src).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        std::process::exit(1);
    });

    let mut translator = Translator::new();

    match translator.load_program(&ast) {
        None => {}
        Some(e) => {
            eprintln!("load error: {e}");
            std::process::exit(1);
        }
    };

    match translator.translate_program() {
        None => {}
        Some(e) => {
            eprintln!("translation failed: {e}");
            std::process::exit(1);
        }
    };


    write_arm64_asm_file(&translator.translated_program, "output.s").unwrap_or_else(|e| {
        eprintln!("failed to write output.s: {e}");
        std::process::exit(1);
    });
}
