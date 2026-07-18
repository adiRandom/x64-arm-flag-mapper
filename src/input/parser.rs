use std::fmt;
use crate::input::ast::{DirectiveArg, DirectiveLine, Line, ParsedMem, ParsedInstruction, ParsedOperand, Size};
use crate::input::token::{SpannedToken, Token};
use crate::input::lexer::Lexer;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
}
 
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at line {}: {}", self.line, self.message)
    }
}

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}   
 
impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Parser { tokens, pos: 0 }
    }
 
    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }
 
    fn peek_at(&self, offset: usize) -> &Token {
        self.tokens.get(self.pos + offset).map(|t| &t.token).unwrap_or(&Token::Eof)
    }
 
    fn cur_line(&self) -> usize {
        self.tokens[self.pos].line
    }
 
    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos].token.clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }
 
    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.bump();
        }
    }
 
    pub fn parse_program(&mut self) -> Result<Vec<Line>, ParseError> {
        let mut lines = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek(), Token::Eof) {
            lines.push(self.parse_line()?);
            match self.peek() {
                Token::Newline | Token::Eof => self.skip_newlines(),
                other => {
                    return Err(ParseError {
                        message: format!("expected end of line, found {other:?}"),
                        line: self.cur_line(),
                    })
                }
            }
        }
        Ok(lines)
    }
 
    fn parse_line(&mut self) -> Result<Line, ParseError> {
        let line_no = self.cur_line();
 
        // Label: `Ident:` or `.local_ident:` (GAS local labels, e.g. `.L1:`).
        if matches!(self.peek_at(1), Token::Colon) {
            match self.peek().clone() {
                Token::Ident(name) => {
                    self.bump();
                    self.bump();
                    return Ok(Line::Label(name));
                }
                Token::Directive(name) => {
                    self.bump();
                    self.bump();
                    return Ok(Line::Label(format!(".{name}")));
                }
                _ => {}
            }
        }
 
        match self.peek().clone() {
            Token::Directive(name) => {
                self.bump();
                self.parse_directive(name, line_no)
            }
            Token::Ident(mnemonic) => {
                self.bump();
                self.parse_instruction(mnemonic, line_no)
            }
            other => Err(ParseError {
                message: format!("expected a label, directive, or mnemonic, found {other:?}"),
                line: line_no,
            }),
        }
    }
 
    fn parse_directive(&mut self, name: String, line_no: usize) -> Result<Line, ParseError> {
        let mut args = Vec::new();
        loop {
            match self.peek().clone() {
                Token::Newline | Token::Eof => break,
                Token::Ident(s) => {
                    self.bump();
                    args.push(DirectiveArg::Ident(s));
                }
                Token::Directive(s) => {
                    self.bump();
                    args.push(DirectiveArg::Ident(format!(".{s}")));
                }
                Token::Number(n) => {
                    self.bump();
                    args.push(DirectiveArg::Number(n));
                }
                Token::StringLit(s) => {
                    self.bump();
                    args.push(DirectiveArg::Str(s));
                }
                Token::Comma => {
                    self.bump();
                }
                other => {
                    return Err(ParseError {
                        message: format!("unexpected token in directive arguments: {other:?}"),
                        line: line_no,
                    })
                }
            }
        }
        Ok(Line::Directive(DirectiveLine { name, args, line: line_no }))
    }
 
    fn parse_instruction(&mut self, mnemonic: String, line_no: usize) -> Result<Line, ParseError> {
        let mut operands = Vec::new();
        if !matches!(self.peek(), Token::Newline | Token::Eof) {
            loop {
                operands.push(self.parse_operand(line_no)?);
                if matches!(self.peek(), Token::Comma) {
                    self.bump();
                    continue;
                }
                break;
            }
        }
        Ok(Line::Instruction(ParsedInstruction { mnemonic, operands, line: line_no }))
    }
 
    fn parse_operand(&mut self, line_no: usize) -> Result<ParsedOperand, ParseError> {
        let size = self.try_parse_size_prefix();
 
        let segment = if let Token::Ident(name) = self.peek().clone() {
            if matches!(self.peek_at(1), Token::Colon) && matches!(self.peek_at(2), Token::LBracket) {
                self.bump();
                self.bump();
                Some(name)
            } else {
                None
            }
        } else {
            None
        };
 
        if matches!(self.peek(), Token::LBracket) {
            return self.parse_memory_operand(size, segment, line_no);
        }
        if size.is_some() || segment.is_some() {
            return Err(ParseError {
                message: "size or segment prefix used without a following memory operand".into(),
                line: line_no,
            });
        }
 
        match self.peek().clone() {
            Token::Number(n) => {
                self.bump();
                Ok(ParsedOperand::Immediate(n))
            }
            Token::Minus => {
                self.bump();
                if let Token::Number(n) = self.peek().clone() {
                    self.bump();
                    Ok(ParsedOperand::Immediate(-n))
                } else {
                    Err(ParseError { message: "expected a number after unary '-'".into(), line: line_no })
                }
            }
            Token::Ident(name) => {
                self.bump();
                if is_known_register(&name) {
                    Ok(ParsedOperand::Register(name))
                } else {
                    Ok(ParsedOperand::LabelRef(name))
                }
            }
            // A local-label reference used as a jump/call target, e.g. `jmp .L1`.
            Token::Directive(name) => {
                self.bump();
                Ok(ParsedOperand::LabelRef(format!(".{name}")))
            }
            other => Err(ParseError { message: format!("unexpected token in operand position: {other:?}"), line: line_no }),
        }
    }
 
    fn try_parse_size_prefix(&mut self) -> Option<Size> {
        let size = if let Token::Ident(name) = self.peek() {
            match name.to_ascii_lowercase().as_str() {
                "byte" => Some(Size::Byte),
                "word" => Some(Size::Word),
                "dword" => Some(Size::Dword),
                "qword" => Some(Size::Qword),
                "xmmword" => Some(Size::Xmmword),
                "ymmword" => Some(Size::Ymmword),
                _ => None,
            }
        } else {
            None
        };
        let size = size?;
        self.bump();
        if let Token::Ident(next) = self.peek() {
            if next.eq_ignore_ascii_case("ptr") {
                self.bump();
            }
        }
        Some(size)
    }
 
    fn parse_memory_operand(
        &mut self,
        size: Option<Size>,
        segment: Option<String>,
        line_no: usize,
    ) -> Result<ParsedOperand, ParseError> {
        self.bump(); // '['
 
        let mut base: Option<String> = None;
        let mut index: Option<String> = None;
        let mut scale: Option<u8> = None;
        let mut disp: i64 = 0;
        let mut rip_relative = false;
        let mut unscaled_regs: Vec<String> = Vec::new();
        let mut sign: i64 = 1;
 
        loop {
            match self.peek().clone() {
                Token::RBracket => {
                    self.bump();
                    break;
                }
                Token::Plus => {
                    self.bump();
                    sign = 1;
                }
                Token::Minus => {
                    self.bump();
                    sign = -1;
                }
                Token::Number(n) => {
                    self.bump();
                    disp += sign * n;
                    sign = 1;
                }
                Token::Ident(name) => {
                    self.bump();
                    if name.eq_ignore_ascii_case("rip") {
                        rip_relative = true;
                        sign = 1;
                        continue;
                    }
                    if matches!(self.peek(), Token::Star) {
                        self.bump();
                        let sc = match self.peek().clone() {
                            Token::Number(n) if [1, 2, 4, 8].contains(&n) => n as u8,
                            other => {
                                return Err(ParseError {
                                    message: format!("invalid scale factor {other:?} (must be 1, 2, 4, or 8)"),
                                    line: line_no,
                                })
                            }
                        };
                        self.bump();
                        if index.is_some() {
                            return Err(ParseError {
                                message: "memory operand has more than one scaled index register".into(),
                                line: line_no,
                            });
                        }
                        index = Some(name);
                        scale = Some(sc);
                    } else {
                        unscaled_regs.push(name);
                    }
                    sign = 1;
                }
                other => {
                    return Err(ParseError {
                        message: format!("unexpected token inside memory operand: {other:?}"),
                        line: line_no,
                    })
                }
            }
        }
 
        // Unscaled registers fill base first, then index (implied scale 1) —
        // this is what `[rax+rbx]` means: base=rax, index=rbx, scale=1.
        for reg in unscaled_regs {
            if base.is_none() {
                base = Some(reg);
            } else if index.is_none() {
                index = Some(reg);
                scale.get_or_insert(1);
            } else {
                return Err(ParseError { message: "memory operand has more than two registers".into(), line: line_no });
            }
        }
 
        if rip_relative && (base.is_some() || index.is_some()) {
            return Err(ParseError {
                message: "rip-relative operand cannot combine with base/index registers".into(),
                line: line_no,
            });
        }
        if rip_relative {
            base = Some("rip".to_string());
        }
 
        Ok(ParsedOperand::Memory(ParsedMem { size, segment, base, index, scale, disp, rip_relative }))
    }
}
 
fn is_known_register(name: &str) -> bool {
    crate::instructions::register::resolve_x64_register(name).is_some()
}
 
// ============================================================
// Convenience entry point
// ============================================================
 
pub fn parse_asm(src: &str) -> Result<Vec<Line>, String> {
    let tokens = Lexer::new(src).tokenize().map_err(|e| e.to_string())?;
    Parser::new(tokens).parse_program().map_err(|e| e.to_string())
}