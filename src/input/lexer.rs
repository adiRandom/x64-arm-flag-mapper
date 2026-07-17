use std::{fmt, iter::Peekable, str::Chars};
use super::token::{SpannedToken, Token};

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}
 
impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lex error at {}:{}: {}", self.line, self.col, self.message)
    }
}

pub struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer { chars: src.chars().peekable(), line: 1, col: 1 }
    }
 
    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next();
        match c {
            Some('\n') => { self.line += 1; self.col = 1; }
            Some(_) => { self.col += 1; }
            None => {}
        }
        c
    }
 
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }
 
    pub fn tokenize(mut self) -> Result<Vec<SpannedToken>, LexError> {
        let mut out = Vec::new();
        loop {
            let tok = self.next_token()?;
            let is_eof = tok.token == Token::Eof;
            out.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(out)
    }
 
    fn next_token(&mut self) -> Result<SpannedToken, LexError> {
        loop {
            let (line, col) = (self.line, self.col);
            match self.peek() {
                None => return Ok(SpannedToken { token: Token::Eof, line, col }),
                Some('\n') => {
                    self.bump();
                    return Ok(SpannedToken { token: Token::Newline, line, col });
                }
                Some(c) if c.is_whitespace() => { self.bump(); }
                Some(';') | Some('#') => self.skip_line_comment(),
                Some('/') => {
                    let mut clone = self.chars.clone();
                    clone.next();
                    if clone.peek() == Some(&'*') {
                        self.bump();
                        self.bump();
                        self.skip_block_comment()?;
                    } else {
                        return Err(LexError { message: "unexpected '/'".into(), line, col });
                    }
                }
                Some(',') => { self.bump(); return Ok(SpannedToken { token: Token::Comma, line, col }); }
                Some(':') => { self.bump(); return Ok(SpannedToken { token: Token::Colon, line, col }); }
                Some('[') => { self.bump(); return Ok(SpannedToken { token: Token::LBracket, line, col }); }
                Some(']') => { self.bump(); return Ok(SpannedToken { token: Token::RBracket, line, col }); }
                Some('+') => { self.bump(); return Ok(SpannedToken { token: Token::Plus, line, col }); }
                Some('-') => { self.bump(); return Ok(SpannedToken { token: Token::Minus, line, col }); }
                Some('*') => { self.bump(); return Ok(SpannedToken { token: Token::Star, line, col }); }
                Some('"') => return self.lex_string(line, col),
                Some('.') => return self.lex_directive(line, col),
                Some(c) if c.is_ascii_digit() => return self.lex_number(line, col),
                Some(c) if is_ident_start(c) => return self.lex_ident(line, col),
                Some(c) => return Err(LexError { message: format!("unexpected character '{c}'"), line, col }),
            }
        }
    }
 
    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
    }
 
    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        loop {
            match self.bump() {
                None => return Err(LexError { message: "unterminated block comment".into(), line: self.line, col: self.col }),
                Some('*') if self.peek() == Some('/') => {
                    self.bump();
                    return Ok(());
                }
                _ => continue,
            }
        }
    }
 
    fn lex_string(&mut self, line: usize, col: usize) -> Result<SpannedToken, LexError> {
        self.bump(); // opening quote
        let mut s = String::new();
        loop {
            match self.bump() {
                None => return Err(LexError { message: "unterminated string literal".into(), line, col }),
                Some('"') => break,
                Some('\\') => match self.bump() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some(other) => s.push(other),
                    None => return Err(LexError { message: "unterminated escape".into(), line, col }),
                },
                Some(c) => s.push(c),
            }
        }
        Ok(SpannedToken { token: Token::StringLit(s), line, col })
    }
 
    fn lex_directive(&mut self, line: usize, col: usize) -> Result<SpannedToken, LexError> {
        self.bump(); // '.'
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if s.is_empty() {
            return Err(LexError { message: "expected a directive name after '.'".into(), line, col });
        }
        Ok(SpannedToken { token: Token::Directive(s), line, col })
    }
 
    fn lex_number(&mut self, line: usize, col: usize) -> Result<SpannedToken, LexError> {
        let mut s = String::new();
        if self.peek() == Some('0') {
            s.push('0');
            self.bump();
            if matches!(self.peek(), Some('x') | Some('X')) {
                self.bump();
                let mut hex = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_hexdigit() {
                        hex.push(c);
                        self.bump();
                    } else {
                        break;
                    }
                }
                let val = i64::from_str_radix(&hex, 16)
                    .map_err(|e| LexError { message: format!("bad hex literal: {e}"), line, col })?;
                return Ok(SpannedToken { token: Token::Number(val), line, col });
            }
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        // NASM-style hex suffix, e.g. `1Ah`.
        if matches!(self.peek(), Some('h') | Some('H')) {
            self.bump();
            let val = i64::from_str_radix(&s, 16)
                .map_err(|e| LexError { message: format!("bad hex literal: {e}"), line, col })?;
            return Ok(SpannedToken { token: Token::Number(val), line, col });
        }
        let val: i64 = s
            .parse()
            .map_err(|e| LexError { message: format!("bad number literal: {e}"), line, col })?;
        Ok(SpannedToken { token: Token::Number(val), line, col })
    }
 
    fn lex_ident(&mut self, line: usize, col: usize) -> Result<SpannedToken, LexError> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        Ok(SpannedToken { token: Token::Ident(s), line, col })
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}
fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}