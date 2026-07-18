mod input;
mod translator;

use std::env;
use std::fs;
use crate::input::parser::parse_asm;
 
fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| "./tests/basic_sample.s".to_string());
 
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

    for instruction in &loaded_x64 {
        println!("{instruction:#?}");
    }
}
 