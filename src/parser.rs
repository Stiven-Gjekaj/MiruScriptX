//! The MiruScriptX parser: turns a token stream into an AST.
//!
//! Statements are parsed with straightforward recursive descent. Expressions
//! use a small Pratt (precedence-climbing) parser: [`Parser::parse_binary`]
//! drives infix operators by binding power, while prefix operators, calls, and
//! indexing are handled by [`Parser::unary`] and [`Parser::postfix`].

use crate::ast::{BinaryOp, Expr, LogicalOp, Stmt, StmtKind, UnaryOp};
use crate::token::{Token, TokenKind};
use crate::MiruError;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    /// Parse a full program (a list of statements) from a token stream.
    pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, MiruError> {
        let mut parser = Parser { tokens, pos: 0 };
        parser.program()
    }

    fn program(&mut self) -> Result<Vec<Stmt>, MiruError> {
        let mut statements = Vec::new();
        self.skip_newlines();
        while !self.is_at_end() {
            statements.push(self.statement()?);
            self.consume_terminator()?;
            self.skip_newlines();
        }
        Ok(statements)
    }

    // --- Statements -------------------------------------------------------

    fn statement(&mut self) -> Result<Stmt, MiruError> {
        let line = self.peek().line;
        match self.peek_kind() {
            TokenKind::Let => self.let_statement(line),
            TokenKind::Return => self.return_statement(line),
            TokenKind::If => self.if_statement(line),
            TokenKind::While => self.while_statement(line),
            TokenKind::For => self.for_statement(line),
            TokenKind::Fn if matches!(self.peek_at_kind(1), Some(TokenKind::Ident(_))) => {
                self.function_statement(line)
            }
            _ => self.expr_or_assign_statement(line),
        }
    }

    fn let_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        self.advance(); // 'let'
        let name = self.expect_identifier("after 'let'")?;
        self.expect(TokenKind::Assign, "after the variable name")?;
        let value = self.expression()?;
        Ok(Stmt::new(StmtKind::Let { name, value }, line))
    }

    fn return_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        self.advance(); // 'return'
        if self.is_statement_end() {
            Ok(Stmt::new(StmtKind::Return(None), line))
        } else {
            let value = self.expression()?;
            Ok(Stmt::new(StmtKind::Return(Some(value)), line))
        }
    }

    fn if_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        self.advance(); // 'if'
        let condition = self.expression()?;
        let then_branch = self.block()?;

        // Allow `else` either on the same line as the closing brace or on a
        // line of its own. We look past newlines and only commit to consuming
        // them if an `else` actually follows.
        let saved = self.pos;
        self.skip_newlines();
        let else_branch = if self.check(&TokenKind::Else) {
            self.advance(); // 'else'
            if self.check(&TokenKind::If) {
                let nested_line = self.peek().line;
                Some(vec![self.if_statement(nested_line)?])
            } else {
                Some(self.block()?)
            }
        } else {
            self.pos = saved;
            None
        };

        Ok(Stmt::new(
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            },
            line,
        ))
    }

    fn while_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        self.advance(); // 'while'
        let condition = self.expression()?;
        let body = self.block()?;
        Ok(Stmt::new(StmtKind::While { condition, body }, line))
    }

    fn for_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        self.advance(); // 'for'
        let name = self.expect_identifier("after 'for'")?;
        self.expect(TokenKind::In, "after the loop variable")?;
        let iterable = self.expression()?;
        let body = self.block()?;
        Ok(Stmt::new(
            StmtKind::For {
                name,
                iterable,
                body,
            },
            line,
        ))
    }

    fn function_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        self.advance(); // 'fn'
        let name = self.expect_identifier("after 'fn'")?;
        let params = self.parse_params()?;
        let body = self.block()?;
        Ok(Stmt::new(
            StmtKind::Function { name, params, body },
            line,
        ))
    }

    fn expr_or_assign_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        let expr = self.expression()?;
        if self.check(&TokenKind::Assign) {
            self.advance(); // '='
            match &expr {
                Expr::Identifier(_) | Expr::Index { .. } => {}
                _ => {
                    return Err(MiruError::new(
                        line,
                        "invalid assignment target (only variables and array elements can be assigned to)",
                    ));
                }
            }
            let value = self.expression()?;
            Ok(Stmt::new(
                StmtKind::Assign {
                    target: expr,
                    value,
                },
                line,
            ))
        } else {
            Ok(Stmt::new(StmtKind::Expr(expr), line))
        }
    }

    fn block(&mut self) -> Result<Vec<Stmt>, MiruError> {
        self.expect(TokenKind::LBrace, "to start a block")?;
        let mut statements = Vec::new();
        self.skip_newlines();
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            statements.push(self.statement()?);
            self.consume_terminator()?;
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace, "to close a block")?;
        Ok(statements)
    }

    fn parse_params(&mut self) -> Result<Vec<String>, MiruError> {
        self.expect(TokenKind::LParen, "to start a parameter list")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                params.push(self.expect_identifier("as a parameter name")?);
                if self.check(&TokenKind::Comma) {
                    self.advance();
                    if self.check(&TokenKind::RParen) {
                        break; // tolerate a trailing comma
                    }
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "to close the parameter list")?;
        Ok(params)
    }

    // --- Expressions (Pratt) ---------------------------------------------

    fn expression(&mut self) -> Result<Expr, MiruError> {
        self.parse_binary(1)
    }

    fn parse_binary(&mut self, min_bp: u8) -> Result<Expr, MiruError> {
        let mut left = self.unary()?;
        while let Some(bp) = Parser::infix_binding_power(self.peek_kind()) {
            if bp < min_bp {
                break;
            }
            let op = self.advance();
            let right = self.parse_binary(bp + 1)?;
            left = Parser::make_infix(&op.kind, left, right);
        }
        Ok(left)
    }

    fn infix_binding_power(kind: &TokenKind) -> Option<u8> {
        match kind {
            TokenKind::Or => Some(1),
            TokenKind::And => Some(2),
            TokenKind::Eq | TokenKind::NotEq => Some(3),
            TokenKind::Lt | TokenKind::Gt | TokenKind::LtEq | TokenKind::GtEq => Some(4),
            TokenKind::Plus | TokenKind::Minus => Some(5),
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Some(6),
            _ => None,
        }
    }

    fn make_infix(kind: &TokenKind, left: Expr, right: Expr) -> Expr {
        let left = Box::new(left);
        let right = Box::new(right);
        match kind {
            TokenKind::Or => Expr::Logical {
                op: LogicalOp::Or,
                left,
                right,
            },
            TokenKind::And => Expr::Logical {
                op: LogicalOp::And,
                left,
                right,
            },
            _ => {
                let op = match kind {
                    TokenKind::Eq => BinaryOp::Equal,
                    TokenKind::NotEq => BinaryOp::NotEqual,
                    TokenKind::Lt => BinaryOp::Less,
                    TokenKind::Gt => BinaryOp::Greater,
                    TokenKind::LtEq => BinaryOp::LessEqual,
                    TokenKind::GtEq => BinaryOp::GreaterEqual,
                    TokenKind::Plus => BinaryOp::Add,
                    TokenKind::Minus => BinaryOp::Subtract,
                    TokenKind::Star => BinaryOp::Multiply,
                    TokenKind::Slash => BinaryOp::Divide,
                    TokenKind::Percent => BinaryOp::Modulo,
                    _ => unreachable!("make_infix called with a non-operator token"),
                };
                Expr::Binary { op, left, right }
            }
        }
    }

    fn unary(&mut self) -> Result<Expr, MiruError> {
        match self.peek_kind() {
            TokenKind::Minus => {
                self.advance();
                let operand = self.unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Negate,
                    operand: Box::new(operand),
                })
            }
            TokenKind::Bang => {
                self.advance();
                let operand = self.unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                })
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Expr, MiruError> {
        let mut expr = self.primary()?;
        loop {
            match self.peek_kind() {
                TokenKind::LParen => {
                    self.advance();
                    let arguments = self.parse_arguments()?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        arguments,
                    };
                }
                TokenKind::LBracket => {
                    self.advance();
                    let index = self.expression()?;
                    self.expect(TokenKind::RBracket, "to close an index")?;
                    expr = Expr::Index {
                        target: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_arguments(&mut self) -> Result<Vec<Expr>, MiruError> {
        let mut arguments = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                arguments.push(self.expression()?);
                if self.check(&TokenKind::Comma) {
                    self.advance();
                    if self.check(&TokenKind::RParen) {
                        break; // tolerate a trailing comma
                    }
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "to close the call")?;
        Ok(arguments)
    }

    fn primary(&mut self) -> Result<Expr, MiruError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Int(n) => {
                self.advance();
                Ok(Expr::Int(n))
            }
            TokenKind::Float(f) => {
                self.advance();
                Ok(Expr::Float(f))
            }
            TokenKind::Str(s) => {
                self.advance();
                Ok(Expr::Str(s))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokenKind::Nil => {
                self.advance();
                Ok(Expr::Nil)
            }
            TokenKind::Ident(name) => {
                self.advance();
                Ok(Expr::Identifier(name))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.expression()?;
                self.expect(TokenKind::RParen, "to close a grouped expression")?;
                Ok(expr)
            }
            TokenKind::LBracket => {
                self.advance();
                let elements = self.parse_array_elements()?;
                Ok(Expr::Array(elements))
            }
            TokenKind::Fn => {
                self.advance();
                let params = self.parse_params()?;
                let body = self.block()?;
                Ok(Expr::Function { params, body })
            }
            other => Err(MiruError::new(
                token.line,
                format!("expected an expression but found {}", other.describe()),
            )),
        }
    }

    fn parse_array_elements(&mut self) -> Result<Vec<Expr>, MiruError> {
        let mut elements = Vec::new();
        if !self.check(&TokenKind::RBracket) {
            loop {
                elements.push(self.expression()?);
                if self.check(&TokenKind::Comma) {
                    self.advance();
                    if self.check(&TokenKind::RBracket) {
                        break; // tolerate a trailing comma
                    }
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RBracket, "to close an array literal")?;
        Ok(elements)
    }

    // --- Token helpers ----------------------------------------------------

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_at_kind(&self, offset: usize) -> Option<&TokenKind> {
        self.tokens.get(self.pos + offset).map(|token| &token.kind)
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if !self.is_at_end() {
            self.pos += 1;
        }
        token
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn check(&self, kind: &TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn is_statement_end(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::RBrace | TokenKind::Eof
        )
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline) {
            self.advance();
        }
    }

    fn consume_terminator(&mut self) -> Result<(), MiruError> {
        match self.peek_kind() {
            TokenKind::Newline => {
                self.advance();
                Ok(())
            }
            TokenKind::RBrace | TokenKind::Eof => Ok(()),
            other => {
                let line = self.peek().line;
                Err(MiruError::new(
                    line,
                    format!("expected end of statement but found {}", other.describe()),
                ))
            }
        }
    }

    fn expect(&mut self, kind: TokenKind, context: &str) -> Result<Token, MiruError> {
        if self.peek_kind() == &kind {
            Ok(self.advance())
        } else {
            let token = self.peek();
            Err(MiruError::new(
                token.line,
                format!(
                    "expected {} {} but found {}",
                    kind.describe(),
                    context,
                    token.kind.describe()
                ),
            ))
        }
    }

    fn expect_identifier(&mut self, context: &str) -> Result<String, MiruError> {
        let token = self.peek().clone();
        if let TokenKind::Ident(name) = token.kind {
            self.advance();
            Ok(name)
        } else {
            Err(MiruError::new(
                token.line,
                format!(
                    "expected an identifier {} but found {}",
                    context,
                    token.kind.describe()
                ),
            ))
        }
    }
}
