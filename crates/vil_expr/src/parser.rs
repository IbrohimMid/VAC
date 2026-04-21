use crate::ast::*;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ParseError {
    #[error("Unexpected end of input")]
    UnexpectedEof,
    #[error("Unexpected token: {0}")]
    UnexpectedToken(String),
    #[error("Invalid number: {0}")]
    InvalidNumber(String),
    #[error("Unterminated string")]
    UnterminatedString,
    #[error("Expected identifier")]
    ExpectedIdentifier,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    Plus,
    Minus,
    Star,
    Slash,
    EqEq,
    BangEq,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Bang,
    Question,
    Colon,
    Dot,
    Comma,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Eof,
}

fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '+' => {
                chars.next();
                tokens.push(Token::Plus);
            }
            '-' => {
                chars.next();
                tokens.push(Token::Minus);
            }
            '*' => {
                chars.next();
                tokens.push(Token::Star);
            }
            '/' => {
                chars.next();
                tokens.push(Token::Slash);
            }
            '?' => {
                chars.next();
                tokens.push(Token::Question);
            }
            ':' => {
                chars.next();
                tokens.push(Token::Colon);
            }
            '.' => {
                chars.next();
                tokens.push(Token::Dot);
            }
            ',' => {
                chars.next();
                tokens.push(Token::Comma);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            '[' => {
                chars.next();
                tokens.push(Token::LBracket);
            }
            ']' => {
                chars.next();
                tokens.push(Token::RBracket);
            }
            '=' => {
                chars.next();
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    tokens.push(Token::EqEq);
                } else {
                    return Err(ParseError::UnexpectedToken("=".to_string()));
                }
            }
            '!' => {
                chars.next();
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    tokens.push(Token::BangEq);
                } else {
                    tokens.push(Token::Bang);
                }
            }
            '<' => {
                chars.next();
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    tokens.push(Token::Le);
                } else {
                    tokens.push(Token::Lt);
                }
            }
            '>' => {
                chars.next();
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    tokens.push(Token::Ge);
                } else {
                    tokens.push(Token::Gt);
                }
            }
            '&' => {
                chars.next();
                if let Some(&'&') = chars.peek() {
                    chars.next();
                    tokens.push(Token::AndAnd);
                } else {
                    return Err(ParseError::UnexpectedToken("&".to_string()));
                }
            }
            '|' => {
                chars.next();
                if let Some(&'|') = chars.peek() {
                    chars.next();
                    tokens.push(Token::OrOr);
                } else {
                    return Err(ParseError::UnexpectedToken("|".to_string()));
                }
            }
            '"' | '\'' => {
                let quote = c;
                chars.next();
                let mut s = String::new();
                let mut escaped = false;
                loop {
                    match chars.next() {
                        Some(c) if escaped => {
                            s.push(c);
                            escaped = false;
                        }
                        Some('\\') => escaped = true,
                        Some(c) if c == quote => break,
                        Some(c) => s.push(c),
                        None => return Err(ParseError::UnterminatedString),
                    }
                }
                tokens.push(Token::Str(s));
            }
            '0'..='9' => {
                let mut num = String::new();
                let mut is_float = false;
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() {
                        num.push(chars.next().unwrap());
                    } else if c == '.' {
                        is_float = true;
                        num.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                if is_float {
                    let f = num.parse().map_err(|_| ParseError::InvalidNumber(num))?;
                    tokens.push(Token::Float(f));
                } else {
                    let i = num.parse().map_err(|_| ParseError::InvalidNumber(num))?;
                    tokens.push(Token::Int(i));
                }
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut ident = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        ident.push(chars.next().unwrap());
                    } else if c == '-' {
                        let mut iter_clone = chars.clone();
                        iter_clone.next(); // consume '-'
                        if let Some(&next_c) = iter_clone.peek() {
                            if next_c.is_ascii_alphabetic() {
                                ident.push(chars.next().unwrap()); // push '-'
                                continue;
                            }
                        }
                        break;
                    } else {
                        break;
                    }
                }
                match ident.as_str() {
                    "true" => tokens.push(Token::Bool(true)),
                    "false" => tokens.push(Token::Bool(false)),
                    "null" => tokens.push(Token::Null),
                    _ => tokens.push(Token::Ident(ident)),
                }
            }
            _ => return Err(ParseError::UnexpectedToken(c.to_string())),
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn next(&mut self) -> &Token {
        if self.pos < self.tokens.len() {
            let p = self.pos;
            self.pos += 1;
            &self.tokens[p]
        } else {
            &Token::Eof
        }
    }

    fn consume(&mut self, expected: Token) -> Result<(), ParseError> {
        if *self.peek() == expected {
            self.next();
            Ok(())
        } else {
            Err(ParseError::UnexpectedToken(format!("{:?}", self.peek())))
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_or()?;
        if *self.peek() == Token::Question {
            self.next();
            let true_branch = self.parse_expr()?;
            self.consume(Token::Colon)?;
            let false_branch = self.parse_expr()?;
            Ok(Expr::Ternary(
                Box::new(expr),
                Box::new(true_branch),
                Box::new(false_branch),
            ))
        } else {
            Ok(expr)
        }
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_and()?;
        while *self.peek() == Token::OrOr {
            self.next();
            let right = self.parse_and()?;
            expr = Expr::BinOp(Box::new(expr), BinOp::Or, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_equality()?;
        while *self.peek() == Token::AndAnd {
            self.next();
            let right = self.parse_equality()?;
            expr = Expr::BinOp(Box::new(expr), BinOp::And, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_comparison()?;
        loop {
            match self.peek() {
                Token::EqEq => {
                    self.next();
                    let right = self.parse_comparison()?;
                    expr = Expr::BinOp(Box::new(expr), BinOp::Eq, Box::new(right));
                }
                Token::BangEq => {
                    self.next();
                    let right = self.parse_comparison()?;
                    expr = Expr::BinOp(Box::new(expr), BinOp::Neq, Box::new(right));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_term()?;
        loop {
            match self.peek() {
                Token::Lt => {
                    self.next();
                    let right = self.parse_term()?;
                    expr = Expr::BinOp(Box::new(expr), BinOp::Lt, Box::new(right));
                }
                Token::Le => {
                    self.next();
                    let right = self.parse_term()?;
                    expr = Expr::BinOp(Box::new(expr), BinOp::Le, Box::new(right));
                }
                Token::Gt => {
                    self.next();
                    let right = self.parse_term()?;
                    expr = Expr::BinOp(Box::new(expr), BinOp::Gt, Box::new(right));
                }
                Token::Ge => {
                    self.next();
                    let right = self.parse_term()?;
                    expr = Expr::BinOp(Box::new(expr), BinOp::Ge, Box::new(right));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_factor()?;
        loop {
            match self.peek() {
                Token::Plus => {
                    self.next();
                    let right = self.parse_factor()?;
                    expr = Expr::BinOp(Box::new(expr), BinOp::Add, Box::new(right));
                }
                Token::Minus => {
                    self.next();
                    let right = self.parse_factor()?;
                    expr = Expr::BinOp(Box::new(expr), BinOp::Sub, Box::new(right));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary()?;
        loop {
            match self.peek() {
                Token::Star => {
                    self.next();
                    let right = self.parse_unary()?;
                    expr = Expr::BinOp(Box::new(expr), BinOp::Mul, Box::new(right));
                }
                Token::Slash => {
                    self.next();
                    let right = self.parse_unary()?;
                    expr = Expr::BinOp(Box::new(expr), BinOp::Div, Box::new(right));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Token::Bang => {
                self.next();
                let right = self.parse_unary()?;
                Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(right)))
            }
            Token::Minus => {
                self.next();
                let right = self.parse_unary()?;
                Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(right)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let mut expr = match self.peek() {
            Token::Int(i) => {
                let i = *i;
                self.next();
                Expr::Literal(Literal::Int(i))
            }
            Token::Float(f) => {
                let f = *f;
                self.next();
                Expr::Literal(Literal::Float(f))
            }
            Token::Str(s) => {
                let s = s.clone();
                self.next();
                Expr::Literal(Literal::Str(s))
            }
            Token::Bool(b) => {
                let b = *b;
                self.next();
                Expr::Literal(Literal::Bool(b))
            }
            Token::Null => {
                self.next();
                Expr::Literal(Literal::Null)
            }
            Token::Ident(id) => {
                let id = id.clone();
                self.next();
                Expr::Ident(id)
            }
            Token::LParen => {
                self.next();
                let inner = self.parse_expr()?;
                self.consume(Token::RParen)?;
                inner
            }
            Token::Eof => return Err(ParseError::UnexpectedEof),
            t => return Err(ParseError::UnexpectedToken(format!("{:?}", t))),
        };

        loop {
            match self.peek() {
                Token::Dot => {
                    self.next();
                    if let Token::Ident(field) = self.peek() {
                        let field = field.clone();
                        self.next();
                        expr = Expr::FieldAccess(Box::new(expr), field);
                    } else {
                        return Err(ParseError::ExpectedIdentifier);
                    }
                }
                Token::LBracket => {
                    self.next();
                    let index = self.parse_expr()?;
                    self.consume(Token::RBracket)?;
                    expr = Expr::Index(Box::new(expr), Box::new(index));
                }
                Token::LParen => {
                    self.next();
                    let mut args = Vec::new();
                    if *self.peek() != Token::RParen {
                        args.push(self.parse_expr()?);
                        while *self.peek() == Token::Comma {
                            self.next();
                            args.push(self.parse_expr()?);
                        }
                    }
                    self.consume(Token::RParen)?;
                    expr = Expr::Call(Box::new(expr), args);
                }
                _ => break,
            }
        }
        Ok(expr)
    }
}

pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let tokens = tokenize(input)?;
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr()?;
    if *parser.peek() != Token::Eof {
        return Err(ParseError::UnexpectedToken(format!(
            "Expected EOF, got {:?}",
            parser.peek()
        )));
    }
    Ok(expr)
}
