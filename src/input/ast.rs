#[derive(Debug, Clone, PartialEq)]
pub enum Size {
    Byte,
    Word,
    Dword,
    Qword,
    Xmmword,
    Ymmword,
}
 
#[derive(Debug, Clone, PartialEq)]
pub struct MemOperand {
    pub size: Option<Size>,
    pub segment: Option<String>,
    pub base: Option<String>,
    pub index: Option<String>,
    pub scale: Option<u8>,
    pub disp: i64,
    pub rip_relative: bool,
}
 
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedOperand {
    Register(String),
    Immediate(i64),
    Memory(MemOperand),
    /// A bare identifier where a register wasn't recognized — most often
    /// a jump/call target. Resolving it against collected label addresses
    /// is a later pass's job, not the parser's.
    LabelRef(String),
}
 
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedInstruction {
    pub mnemonic: String,
    pub operands: Vec<ParsedOperand>,
    pub line: usize,
}
 
#[derive(Debug, Clone, PartialEq)]
pub enum DirectiveArg {
    Ident(String),
    Number(i64),
    Str(String),
}
 
#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveLine {
    pub name: String,
    pub args: Vec<DirectiveArg>,
    pub line: usize,
}
 
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Label(String),
    Directive(DirectiveLine),
    Instruction(ParsedInstruction),
}