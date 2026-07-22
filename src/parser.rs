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
            let stmt = self.statement()?;
            if !Parser::ends_with_block(&stmt.kind) {
                self.consume_terminator()?;
            }
            self.skip_newlines();
            statements.push(stmt);
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
        Ok(Stmt::new(StmtKind::Function { name, params, body }, line))
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
            let stmt = self.statement()?;
            if !Parser::ends_with_block(&stmt.kind) {
                self.consume_terminator()?;
            }
            self.skip_newlines();
            statements.push(stmt);
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

    /// Statements that end in a `}` (a block) do not need a following newline or
    /// `;` before the next statement, so one-liners like
    /// `if c { return 1 } return 2` parse cleanly.
    fn ends_with_block(kind: &StmtKind) -> bool {
        matches!(
            kind,
            StmtKind::If { .. }
                | StmtKind::While { .. }
                | StmtKind::For { .. }
                | StmtKind::Function { .. }
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinaryOp, Expr, LogicalOp, StmtKind, UnaryOp};
    use crate::lexer::Lexer;

    fn parse_program(source: &str) -> Vec<Stmt> {
        let tokens = Lexer::tokenize(source).expect("source should tokenize");
        Parser::parse(tokens).expect("source should parse")
    }

    fn parse_expr(source: &str) -> Expr {
        let mut statements = parse_program(source);
        assert_eq!(statements.len(), 1, "expected a single statement");
        match statements.remove(0).kind {
            StmtKind::Expr(expr) => expr,
            other => panic!("expected an expression statement, found {other:?}"),
        }
    }

    #[test]
    fn parses_let_statement() {
        let statements = parse_program("let x = 1 + 2");
        match &statements[0].kind {
            StmtKind::Let { name, value } => {
                assert_eq!(name, "x");
                assert_eq!(
                    *value,
                    Expr::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Int(1)),
                        right: Box::new(Expr::Int(2)),
                    }
                );
            }
            other => panic!("expected a let statement, found {other:?}"),
        }
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(
            parse_expr("1 + 2 * 3"),
            Expr::Binary {
                op: BinaryOp::Add,
                left: Box::new(Expr::Int(1)),
                right: Box::new(Expr::Binary {
                    op: BinaryOp::Multiply,
                    left: Box::new(Expr::Int(2)),
                    right: Box::new(Expr::Int(3)),
                }),
            }
        );
    }

    #[test]
    fn subtraction_is_left_associative() {
        assert_eq!(
            parse_expr("1 - 2 - 3"),
            Expr::Binary {
                op: BinaryOp::Subtract,
                left: Box::new(Expr::Binary {
                    op: BinaryOp::Subtract,
                    left: Box::new(Expr::Int(1)),
                    right: Box::new(Expr::Int(2)),
                }),
                right: Box::new(Expr::Int(3)),
            }
        );
    }

    #[test]
    fn unary_negation_binds_tighter_than_multiplication() {
        assert_eq!(
            parse_expr("-2 * 3"),
            Expr::Binary {
                op: BinaryOp::Multiply,
                left: Box::new(Expr::Unary {
                    op: UnaryOp::Negate,
                    operand: Box::new(Expr::Int(2)),
                }),
                right: Box::new(Expr::Int(3)),
            }
        );
    }

    #[test]
    fn logical_and_binds_tighter_than_or() {
        assert_eq!(
            parse_expr("a || b && c"),
            Expr::Logical {
                op: LogicalOp::Or,
                left: Box::new(Expr::Identifier("a".to_string())),
                right: Box::new(Expr::Logical {
                    op: LogicalOp::And,
                    left: Box::new(Expr::Identifier("b".to_string())),
                    right: Box::new(Expr::Identifier("c".to_string())),
                }),
            }
        );
    }

    #[test]
    fn parses_call_then_index() {
        assert_eq!(
            parse_expr("f(1)[0]"),
            Expr::Index {
                target: Box::new(Expr::Call {
                    callee: Box::new(Expr::Identifier("f".to_string())),
                    arguments: vec![Expr::Int(1)],
                }),
                index: Box::new(Expr::Int(0)),
            }
        );
    }

    #[test]
    fn parses_array_literal() {
        assert_eq!(
            parse_expr("[1, 2, 3]"),
            Expr::Array(vec![Expr::Int(1), Expr::Int(2), Expr::Int(3)])
        );
    }

    #[test]
    fn parses_function_declaration() {
        let statements = parse_program("fn add(a, b) {\n  return a + b\n}");
        match &statements[0].kind {
            StmtKind::Function { name, params, body } => {
                assert_eq!(name, "add");
                assert_eq!(params, &vec!["a".to_string(), "b".to_string()]);
                assert_eq!(body.len(), 1);
                assert!(matches!(body[0].kind, StmtKind::Return(Some(_))));
            }
            other => panic!("expected a function declaration, found {other:?}"),
        }
    }

    #[test]
    fn parses_if_else_if_chain() {
        let statements = parse_program("if a { 1 } else if b { 2 } else { 3 }");
        match &statements[0].kind {
            StmtKind::If {
                else_branch: Some(else_statements),
                ..
            } => {
                assert_eq!(else_statements.len(), 1);
                assert!(matches!(else_statements[0].kind, StmtKind::If { .. }));
            }
            other => panic!("expected an if with an else branch, found {other:?}"),
        }
    }

    #[test]
    fn parses_for_in_loop() {
        let statements = parse_program("for n in names {\n  print(n)\n}");
        match &statements[0].kind {
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                assert_eq!(name, "n");
                assert_eq!(*iterable, Expr::Identifier("names".to_string()));
                assert_eq!(body.len(), 1);
            }
            other => panic!("expected a for loop, found {other:?}"),
        }
    }

    #[test]
    fn parses_assignment() {
        let statements = parse_program("x = 5");
        match &statements[0].kind {
            StmtKind::Assign { target, value } => {
                assert_eq!(*target, Expr::Identifier("x".to_string()));
                assert_eq!(*value, Expr::Int(5));
            }
            other => panic!("expected an assignment, found {other:?}"),
        }
    }

    #[test]
    fn reports_missing_expression() {
        let tokens = Lexer::tokenize("let x =").expect("lexes");
        let err = Parser::parse(tokens).unwrap_err();
        assert!(err.message.contains("expected an expression"));
    }

    #[test]
    fn a_statement_may_follow_a_block_on_one_line() {
        let statements = parse_program("fn f(n) { if n < 1 { return 0 } return 1 }");
        match &statements[0].kind {
            StmtKind::Function { body, .. } => assert_eq!(body.len(), 2),
            other => panic!("expected a function, found {other:?}"),
        }
    }
}
