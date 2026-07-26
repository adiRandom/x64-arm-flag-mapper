use super::instruction::Instruction;
use crate::input::ast::DirectiveArg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Directive {
    pub name: String,
    pub args: Vec<DirectiveArg>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TranslationStatement {
    // The instruction and the coresponding x64 line index
    Instruction(Instruction, usize),
    Label(Label),
    Directive(Directive),
}
