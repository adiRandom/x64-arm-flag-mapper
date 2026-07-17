#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),     // mnemonics, register names, labels, size keywords
    Directive(String), // ".text" -> "text"
    Number(i64),
    StringLit(String),
    Comma,
    Colon,
    LBracket,
    RBracket,
    Plus,
    Minus,
    Star,
    Newline,
    Eof,
}
 
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub line: usize,
    pub col: usize,
}