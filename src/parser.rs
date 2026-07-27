//! The MiruScriptX parser: turns a token stream into an AST.
//!
//! Statements are parsed with straightforward recursive descent. Expressions
//! use a small Pratt (precedence-climbing) parser: [`Parser::parse_binary`]
//! drives infix operators by binding power, while prefix operators, calls, and
//! indexing are handled by [`Parser::unary`] and [`Parser::postfix`].

use crate::ast::{BinaryOp, Expr, ExprKind, LogicalOp, Stmt, StmtKind, UnaryOp};
use crate::token::{Token, TokenKind};
use crate::MiruError;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    loop_depth: usize,
}

impl Parser {
    /// Parse a full program (a list of statements) from a token stream.
    pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, MiruError> {
        let mut parser = Parser {
            tokens,
            pos: 0,
            loop_depth: 0,
        };
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
            TokenKind::Import => self.import_statement(line),
            TokenKind::Let => self.let_statement(line),
            TokenKind::Return => self.return_statement(line),
            TokenKind::If => self.if_statement(line),
            TokenKind::While => self.while_statement(line),
            TokenKind::For => self.for_statement(line),
            TokenKind::Break => self.break_statement(line),
            TokenKind::Continue => self.continue_statement(line),
            TokenKind::Fn if matches!(self.peek_at_kind(1), Some(TokenKind::Ident(_))) => {
                self.function_statement(line)
            }
            _ => self.expr_or_assign_statement(line),
        }
    }

    /// `import "./math.miru" as math`.
    ///
    /// The path is a string literal rather than a bare word so it can hold any
    /// filename, and the alias is required rather than inferred from it: a
    /// reader should be able to tell where a name came from without working out
    /// what a path would have been shortened to.
    fn import_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        self.advance();
        let literal = self.peek().clone();
        let spec = match &literal.kind {
            TokenKind::Str(spec) => spec.clone(),
            other => {
                return Err(MiruError::with_column(
                    literal.line,
                    literal.column,
                    format!(
                        "expected a quoted path after 'import' but found {}",
                        other.describe()
                    ),
                ))
            }
        };
        self.advance();
        self.expect(TokenKind::As, "after an import path")?;
        let name = self.peek().clone();
        let alias = match &name.kind {
            TokenKind::Ident(alias) => alias.clone(),
            other => {
                return Err(MiruError::with_column(
                    name.line,
                    name.column,
                    format!("expected a name after 'as' but found {}", other.describe()),
                ))
            }
        };
        self.advance();
        Ok(Stmt::new(
            StmtKind::Import {
                spec,
                alias,
                column: literal.column,
            },
            line,
        ))
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
        let body = self.loop_body()?;
        Ok(Stmt::new(StmtKind::While { condition, body }, line))
    }

    fn for_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        self.advance(); // 'for'
        let name = self.expect_identifier("after 'for'")?;
        self.expect(TokenKind::In, "after the loop variable")?;
        let iterable = self.expression()?;
        let body = self.loop_body()?;
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
        let body = self.function_body()?;
        Ok(Stmt::new(StmtKind::Function { name, params, body }, line))
    }

    fn expr_or_assign_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        let expr = self.expression()?;
        if self.check(&TokenKind::Assign) {
            self.advance(); // '='
            match &expr.kind {
                ExprKind::Identifier(_) | ExprKind::Index { .. } => {}
                _ => {
                    return Err(MiruError::with_column(
                        expr.line,
                        expr.column,
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
            // Only `block` can tell that a statement is nested, and an import is
            // a file-level thing: one is resolved before the file compiles, so
            // an import inside a branch or a loop could not mean what it looks
            // like it means.
            if let StmtKind::Import { column, .. } = stmt.kind {
                return Err(MiruError::with_column(
                    stmt.line,
                    column,
                    "import must appear at the top level of a file",
                ));
            }
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

    fn break_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        let column = self.peek().column;
        self.advance(); // 'break'
        if self.loop_depth == 0 {
            return Err(MiruError::with_column(
                line,
                column,
                "break outside of a loop",
            ));
        }
        Ok(Stmt::new(StmtKind::Break, line))
    }

    fn continue_statement(&mut self, line: usize) -> Result<Stmt, MiruError> {
        let column = self.peek().column;
        self.advance(); // 'continue'
        if self.loop_depth == 0 {
            return Err(MiruError::with_column(
                line,
                column,
                "continue outside of a loop",
            ));
        }
        Ok(Stmt::new(StmtKind::Continue, line))
    }

    /// Parse a loop body, marking that `break` and `continue` are allowed inside.
    fn loop_body(&mut self) -> Result<Vec<Stmt>, MiruError> {
        self.loop_depth += 1;
        let body = self.block();
        self.loop_depth -= 1;
        body
    }

    /// Parse a function body. A `break` or `continue` cannot target a loop
    /// outside the function, so loop depth is reset while parsing the body.
    fn function_body(&mut self) -> Result<Vec<Stmt>, MiruError> {
        let saved = self.loop_depth;
        self.loop_depth = 0;
        let body = self.block();
        self.loop_depth = saved;
        body
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
            left = Parser::make_infix(&op, left, right);
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

    fn make_infix(op: &Token, left: Expr, right: Expr) -> Expr {
        let left = Box::new(left);
        let right = Box::new(right);
        let kind = match &op.kind {
            TokenKind::Or => ExprKind::Logical {
                op: LogicalOp::Or,
                left,
                right,
            },
            TokenKind::And => ExprKind::Logical {
                op: LogicalOp::And,
                left,
                right,
            },
            _ => {
                let binary_op = match &op.kind {
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
                ExprKind::Binary {
                    op: binary_op,
                    left,
                    right,
                }
            }
        };
        Expr::new(kind, op.line, op.column)
    }

    fn unary(&mut self) -> Result<Expr, MiruError> {
        match self.peek_kind() {
            TokenKind::Minus => {
                let op = self.advance();
                let operand = self.unary()?;
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnaryOp::Negate,
                        operand: Box::new(operand),
                    },
                    op.line,
                    op.column,
                ))
            }
            TokenKind::Bang => {
                let op = self.advance();
                let operand = self.unary()?;
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                    op.line,
                    op.column,
                ))
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
                    let (line, column) = (expr.line, expr.column);
                    expr = Expr::new(
                        ExprKind::Call {
                            callee: Box::new(expr),
                            arguments,
                        },
                        line,
                        column,
                    );
                }
                TokenKind::Dot => {
                    self.advance();
                    // The access carries the *field's* position rather than the
                    // target's, following what Call and Index do: an expression
                    // reports at the part most likely to be at fault, which for
                    // `config.tiemout` is the name. The target's position rides
                    // along in the opcode's operand byte.
                    let field = self.peek().clone();
                    let name = match &field.kind {
                        TokenKind::Ident(name) => name.clone(),
                        other => {
                            return Err(MiruError::with_column(
                                field.line,
                                field.column,
                                format!(
                                    "expected a field name after '.' but found {}",
                                    other.describe()
                                ),
                            ))
                        }
                    };
                    self.advance();
                    expr = Expr::new(
                        ExprKind::Field {
                            target: Box::new(expr),
                            name,
                        },
                        field.line,
                        field.column,
                    );
                }
                TokenKind::LBracket => {
                    self.advance();
                    let index = self.expression()?;
                    self.expect(TokenKind::RBracket, "to close an index")?;
                    let (line, column) = (expr.line, expr.column);
                    expr = Expr::new(
                        ExprKind::Index {
                            target: Box::new(expr),
                            index: Box::new(index),
                        },
                        line,
                        column,
                    );
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
        let line = token.line;
        let column = token.column;
        match token.kind {
            TokenKind::Int(n) => {
                self.advance();
                Ok(Expr::new(ExprKind::Int(n), line, column))
            }
            TokenKind::Float(f) => {
                self.advance();
                Ok(Expr::new(ExprKind::Float(f), line, column))
            }
            TokenKind::Str(s) => {
                self.advance();
                Ok(Expr::new(ExprKind::Str(s), line, column))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::new(ExprKind::Bool(true), line, column))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::new(ExprKind::Bool(false), line, column))
            }
            TokenKind::Nil => {
                self.advance();
                Ok(Expr::new(ExprKind::Nil, line, column))
            }
            TokenKind::Ident(name) => {
                self.advance();
                Ok(Expr::new(ExprKind::Identifier(name), line, column))
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
                Ok(Expr::new(ExprKind::Array(elements), line, column))
            }
            TokenKind::LBrace => {
                self.advance();
                self.parse_map(line, column)
            }
            TokenKind::Fn => {
                self.advance();
                let params = self.parse_params()?;
                let body = self.function_body()?;
                Ok(Expr::new(ExprKind::Function { params, body }, line, column))
            }
            other => Err(MiruError::with_column(
                line,
                column,
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

    fn parse_map(&mut self, line: usize, column: usize) -> Result<Expr, MiruError> {
        // A brace restores newline significance, because blocks need it, so a
        // map literal skips newlines between its entries itself. That was true
        // before v0.8 made braces restore it inside a group too, which is why
        // that change did not disturb map literals.
        let mut entries = Vec::new();
        self.skip_newlines();
        if !self.check(&TokenKind::RBrace) {
            loop {
                let key = self.expression()?;
                self.expect(TokenKind::Colon, "after a map key")?;
                let value = self.expression()?;
                entries.push((key, value));
                self.skip_newlines();
                if self.check(&TokenKind::Comma) {
                    self.advance();
                    self.skip_newlines();
                    if self.check(&TokenKind::RBrace) {
                        break; // tolerate a trailing comma
                    }
                } else {
                    break;
                }
            }
        }
        self.expect(TokenKind::RBrace, "to close a map literal")?;
        Ok(Expr::new(ExprKind::Map(entries), line, column))
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
                let token = self.peek();
                Err(MiruError::with_column(
                    token.line,
                    token.column,
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
            Err(MiruError::with_column(
                token.line,
                token.column,
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
            Err(MiruError::with_column(
                token.line,
                token.column,
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
    use crate::ast::{BinaryOp, Expr, ExprKind, LogicalOp, StmtKind, UnaryOp};
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

    /// Wrap an expression kind with dummy position, for comparing tree shape.
    /// `Expr`'s `PartialEq` ignores positions, so these match parsed output.
    fn e(kind: ExprKind) -> Expr {
        Expr::new(kind, 0, 0)
    }

    #[test]
    fn syntax_error_carries_a_column() {
        // 'let' must be followed by a name; the '=' sits at column 5.
        let tokens = Lexer::tokenize("let = 1").expect("source should tokenize");
        let err = Parser::parse(tokens).unwrap_err();
        assert_eq!(err.line, 1);
        assert_eq!(err.column, 5);
    }

    #[test]
    fn parses_let_statement() {
        let statements = parse_program("let x = 1 + 2");
        match &statements[0].kind {
            StmtKind::Let { name, value } => {
                assert_eq!(name, "x");
                assert_eq!(
                    *value,
                    e(ExprKind::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(e(ExprKind::Int(1))),
                        right: Box::new(e(ExprKind::Int(2))),
                    })
                );
            }
            other => panic!("expected a let statement, found {other:?}"),
        }
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(
            parse_expr("1 + 2 * 3"),
            e(ExprKind::Binary {
                op: BinaryOp::Add,
                left: Box::new(e(ExprKind::Int(1))),
                right: Box::new(e(ExprKind::Binary {
                    op: BinaryOp::Multiply,
                    left: Box::new(e(ExprKind::Int(2))),
                    right: Box::new(e(ExprKind::Int(3))),
                })),
            })
        );
    }

    #[test]
    fn subtraction_is_left_associative() {
        assert_eq!(
            parse_expr("1 - 2 - 3"),
            e(ExprKind::Binary {
                op: BinaryOp::Subtract,
                left: Box::new(e(ExprKind::Binary {
                    op: BinaryOp::Subtract,
                    left: Box::new(e(ExprKind::Int(1))),
                    right: Box::new(e(ExprKind::Int(2))),
                })),
                right: Box::new(e(ExprKind::Int(3))),
            })
        );
    }

    #[test]
    fn unary_negation_binds_tighter_than_multiplication() {
        assert_eq!(
            parse_expr("-2 * 3"),
            e(ExprKind::Binary {
                op: BinaryOp::Multiply,
                left: Box::new(e(ExprKind::Unary {
                    op: UnaryOp::Negate,
                    operand: Box::new(e(ExprKind::Int(2))),
                })),
                right: Box::new(e(ExprKind::Int(3))),
            })
        );
    }

    #[test]
    fn logical_and_binds_tighter_than_or() {
        assert_eq!(
            parse_expr("a || b && c"),
            e(ExprKind::Logical {
                op: LogicalOp::Or,
                left: Box::new(e(ExprKind::Identifier("a".to_string()))),
                right: Box::new(e(ExprKind::Logical {
                    op: LogicalOp::And,
                    left: Box::new(e(ExprKind::Identifier("b".to_string()))),
                    right: Box::new(e(ExprKind::Identifier("c".to_string()))),
                })),
            })
        );
    }

    #[test]
    fn parses_call_then_index() {
        assert_eq!(
            parse_expr("f(1)[0]"),
            e(ExprKind::Index {
                target: Box::new(e(ExprKind::Call {
                    callee: Box::new(e(ExprKind::Identifier("f".to_string()))),
                    arguments: vec![e(ExprKind::Int(1))],
                })),
                index: Box::new(e(ExprKind::Int(0))),
            })
        );
    }

    #[test]
    fn parses_array_literal() {
        assert_eq!(
            parse_expr("[1, 2, 3]"),
            e(ExprKind::Array(vec![
                e(ExprKind::Int(1)),
                e(ExprKind::Int(2)),
                e(ExprKind::Int(3)),
            ]))
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
                assert_eq!(*iterable, e(ExprKind::Identifier("names".to_string())));
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
                assert_eq!(*target, e(ExprKind::Identifier("x".to_string())));
                assert_eq!(*value, e(ExprKind::Int(5)));
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

    #[test]
    fn parses_break_and_continue_in_a_loop() {
        let statements = parse_program("while true {\n  continue\n  break\n}");
        match &statements[0].kind {
            StmtKind::While { body, .. } => {
                assert!(matches!(body[0].kind, StmtKind::Continue));
                assert!(matches!(body[1].kind, StmtKind::Break));
            }
            other => panic!("expected a while loop, found {other:?}"),
        }
    }

    #[test]
    fn break_outside_a_loop_is_an_error() {
        let tokens = Lexer::tokenize("break").expect("lexes");
        let err = Parser::parse(tokens).unwrap_err();
        assert!(err.message.contains("break outside of a loop"));
    }

    #[test]
    fn break_cannot_target_a_loop_outside_a_function() {
        let tokens =
            Lexer::tokenize("for i in [1] {\n  fn f() {\n    break\n  }\n}").expect("lexes");
        let err = Parser::parse(tokens).unwrap_err();
        assert!(err.message.contains("break outside of a loop"));
    }

    #[test]
    fn parses_map_literal() {
        assert_eq!(
            parse_expr("{\"a\": 1, \"b\": 2}"),
            e(ExprKind::Map(vec![
                (e(ExprKind::Str("a".to_string())), e(ExprKind::Int(1))),
                (e(ExprKind::Str("b".to_string())), e(ExprKind::Int(2))),
            ]))
        );
    }

    #[test]
    fn parses_empty_map() {
        assert_eq!(parse_expr("{}"), e(ExprKind::Map(vec![])));
    }
}
