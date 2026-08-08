//! The MiruScriptX parser: turns a token stream into an AST.
//!
//! Statements are parsed with straightforward recursive descent. Expressions
//! use a small Pratt (precedence-climbing) parser: [`Parser::parse_binary`]
//! drives infix operators by binding power, while prefix operators, calls, and
//! indexing are handled by [`Parser::unary`] and [`Parser::postfix`].

use crate::ast::{BinaryOp, Expr, ExprKind, LogicalOp, Stmt, StmtKind, UnaryOp};
use crate::token::{FStringPart, Token, TokenKind};
use crate::MiruError;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    loop_depth: usize,
    depth: usize,
}

impl Parser {
    /// How deeply one program may nest.
    ///
    /// Without a limit, deep source overflows the Rust stack and aborts the
    /// process: no message, no caret, and nothing `try` can catch.
    ///
    /// This counts **how far the parser calls itself**. `[[[ .. ]]]` descends
    /// one frame per bracket and overflows on the way down, before a tree
    /// exists to measure. [`Parser::enter`] counts it.
    ///
    /// A tall tree is the other half of the same defect and has its own limit,
    /// [`Parser::MAX_HEIGHT`], because it costs a different amount of stack.
    /// One does not imply the other: `1 + 1 + 1 ..` is one frame and one loop
    /// however long it runs, and nesting builds no tall tree at all.
    ///
    /// The figure comes from the tightest configuration measured, not the
    /// roomiest, because the promise is that deep source reports rather than
    /// aborts, and a promise that holds only on the main thread is not one.
    ///
    /// It is measured against a stack this project chooses rather than one it
    /// inherits. `miru` starts a thread with 64 MiB (`STACK_SIZE` in
    /// `src/main.rs`) and the WebAssembly build links a 16 MiB shadow stack
    /// (`.cargo/config.toml`), because a language whose grammar depends on
    /// which thread it runs on does not have a grammar. Nested maps, the most
    /// expensive construct, survived to:
    ///
    /// | Build                  | Depth  |
    /// | ---------------------- | ------ |
    /// | Debug, 64 MiB thread   | 3000   |
    /// | Release, 64 MiB thread | 12000+ |
    ///
    /// 1000 leaves a margin of three on the tightest of those.
    ///
    /// It also clears what 1.0 managed. 1.0 had no limit, so it simply aborted
    /// wherever the stack ran out, and the release binary reached 917 levels of
    /// bracket nesting on the 1 MiB shadow stack the playground had. 1000 is
    /// above that, so no program that nested successfully anywhere in 1.0 is
    /// refused here.
    ///
    /// **An embedder calling the library directly gets its own thread's stack**,
    /// which is 2 MiB by default and will not support this limit. The `miru`
    /// binary and the playground both provide the stack; anything else has to
    /// ask for it. `docs/stability.md` says so, and the deep tests in
    /// `tests/golden.rs` spawn a thread rather than assume one.
    ///
    /// For scale, the deepest nesting in this project's own example programs is
    /// four. Before 1.1 there was no limit at all, and the process aborted
    /// somewhere past 600. [`crate::value::Value`] limits nesting at 256 for
    /// comparing and printing, which is a different figure for a different
    /// thing: a loop can build a value far deeper than the source that made it.
    pub const MAX_NESTING: usize = 1000;

    /// How tall the tree for one expression may get.
    ///
    /// `1 + 1 + 1 ..` is one parser frame and one loop however long it runs, so
    /// [`Parser::MAX_NESTING`] never sees it, but it leaves a tree as tall as
    /// the chain is long. Nothing overflows until something walks that tree:
    /// the compiler, the formatter, or `miru disasm`. [`Expr::height`] carries
    /// the figure and [`Parser::checked`] tests it as each level is added,
    /// rather than on the finished expression, because a tree too tall to walk
    /// was until 1.1 also too tall to release, and rejecting it after building
    /// it aborted in the destructor instead.
    ///
    /// **This is ten times [`Parser::MAX_NESTING`] because height is ten times
    /// cheaper.** Both were one figure at first, and 1000 was right for nesting
    /// and much too strict for a chain. Measured on this project's own release
    /// binary, with the height check lifted, on a 16 MiB stack, which is the
    /// smallest of any build that ships:
    ///
    /// | Walk           | Height reached |
    /// | -------------- | -------------- |
    /// | `miru run`     | 80000          |
    /// | `miru disasm`  | 80000          |
    /// | `miru fmt`     | 61000          |
    ///
    /// The formatter is the binding one, so 10000 keeps a margin of six there.
    /// Releasing the tree used to be a fourth walk and the tightest of them;
    /// it is iterative as of 1.1 and no longer constrains this.
    ///
    /// The lower bound comes from 1.0 rather than from the stack. 1.0 had no
    /// limit and its release binary summed 4959 terms on the 1 MiB stack the
    /// playground had, and 40255 on an 8 MiB main thread. A program has to
    /// clear the first of those to have worked everywhere 1.0 ran, and 10000
    /// is twice it. Between 10000 and 40255 sits a band that 1.0 ran on a large
    /// native stack and never in a browser; that is refused here, and a chain
    /// of ten thousand terms is not something a person writes.
    pub const MAX_HEIGHT: usize = 10_000;

    /// The most errors one parse reports.
    ///
    /// A wall of errors is as unhelpful as one. Past this the parse stops, and
    /// whoever reports them says how many there were.
    pub const MAX_ERRORS: usize = 20;

    /// How many statements may fail in a row before the parse gives up.
    ///
    /// **This bounds a cascade whose cause the parser cannot repair.** The
    /// lexer suppresses newlines inside `(` and `[` so an expression can span
    /// lines, which means an unclosed bracket removes every statement
    /// separator in the rest of the file. Nothing after it can end a statement,
    /// so every following statement fails with the same complaint, and one
    /// missing `)` produced twenty errors before this existed.
    ///
    /// That information is gone before the parser runs, so recovery cannot get
    /// it back. What it can do is notice that it is not recovering: no
    /// statement has parsed since the last error, so the errors after the first
    /// are consequences of it rather than separate mistakes.
    ///
    /// Three rather than one, because three mistakes really can sit on
    /// consecutive lines. A successfully parsed statement resets the count, so
    /// mistakes separated by working code all report however many there are.
    const MAX_CONSECUTIVE: usize = 3;

    /// Parse a full program (a list of statements) from a token stream.
    ///
    /// Gives **every** syntax error it can find, not only the first. A file
    /// with four mistakes reports four, so fixing them takes one run rather
    /// than four.
    pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, Vec<MiruError>> {
        let mut parser = Parser {
            tokens,
            pos: 0,
            loop_depth: 0,
            depth: 0,
        };
        parser.program()
    }

    /// Count one level of nesting, and refuse past the limit.
    ///
    /// Every `enter` has a matching `leave` on the path that succeeds, and each
    /// caller runs its `leave` on the failing path too, so an error raised
    /// below leaves the count where it started.
    ///
    /// **The one path that does not is this one.** Refusing here returns before
    /// any `leave`, so the count stays high. That used to be harmless, because
    /// an error ended the parse; since 1.7 the parser recovers and carries on,
    /// and `Parser::program` handles it by checking that the count came back to
    /// zero before recovering. It does not after this error, so this error
    /// stops the parse, which is what the old comment assumed of every error.
    fn enter(&mut self, line: usize, column: usize) -> Result<(), MiruError> {
        self.depth += 1;
        if self.depth > Parser::MAX_NESTING {
            return Err(MiruError::with_column(
                line,
                column,
                "the program is nested too deeply".to_string(),
            ));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// Refuse an expression that is too tall to walk.
    ///
    /// Called on each level as it is added, never on the finished expression.
    ///
    /// The message is not the one [`Parser::enter`] gives, because this is not
    /// the same fault. What trips this is a chain: a sum of thousands of terms,
    /// or `a[0][0][0] ..`, where nothing is nested inside anything and telling
    /// the reader it is would send them looking for brackets that are not
    /// there.
    fn checked(&self, expr: Expr) -> Result<Expr, MiruError> {
        if expr.height > Parser::MAX_HEIGHT {
            return Err(MiruError::with_column(
                expr.line,
                expr.column,
                "the expression is too long".to_string(),
            ));
        }
        Ok(expr)
    }

    fn program(&mut self) -> Result<Vec<Stmt>, Vec<MiruError>> {
        let mut statements = Vec::new();
        let mut errors: Vec<MiruError> = Vec::new();
        let mut consecutive = 0;
        self.skip_newlines();
        while !self.is_at_end() {
            let started_at = self.pos;
            match self.one_statement() {
                Ok(stmt) => {
                    self.skip_newlines();
                    statements.push(stmt);
                    consecutive = 0;
                }
                Err(error) => {
                    // Not two reports of one mistake. Recovery that restarts
                    // just before the token it failed on would say the same
                    // thing again, and ten errors for one missing brace is
                    // worse than the one error this replaces.
                    if errors
                        .last()
                        .is_none_or(|last| (last.line, last.column) != (error.line, error.column))
                    {
                        errors.push(error);
                    }

                    // **The counter has to have come back.** Every `enter` in
                    // this parser is paired with a `leave` that runs on the
                    // failing path too, so an error raised inside a statement
                    // leaves the depth where it started and recovery is safe.
                    // The one exception is `enter` itself refusing, which
                    // returns before its own `leave`: that is the nesting
                    // limit, and carrying on from there would leave the count
                    // high for the rest of the file and report `the program is
                    // nested too deeply` somewhere nothing is nested.
                    //
                    // Checking the invariant rather than the message is what
                    // makes this hold for whatever the parser does later. A
                    // new unbalanced path stops recovery instead of poisoning
                    // it.
                    consecutive += 1;
                    if self.depth != 0
                        || errors.len() >= Parser::MAX_ERRORS
                        || consecutive >= Parser::MAX_CONSECUTIVE
                    {
                        break;
                    }

                    self.synchronise();
                    // Recovery must consume something. A synchronisation point
                    // that is already under the cursor would otherwise fail the
                    // same statement forever.
                    if self.pos == started_at {
                        self.advance();
                    }
                    self.skip_newlines();
                }
            }
        }
        if errors.is_empty() {
            Ok(statements)
        } else {
            Err(errors)
        }
    }

    /// One statement and the terminator it needs, which is the unit recovery
    /// restarts at.
    fn one_statement(&mut self) -> Result<Stmt, MiruError> {
        let stmt = self.statement()?;
        if !Parser::ends_with_block(&stmt.kind) {
            self.consume_terminator()?;
        }
        Ok(stmt)
    }

    /// Skip tokens until a place where parsing can start again with
    /// confidence.
    ///
    /// A statement is the unit a program repeats, so the points are the ones
    /// that begin or end one: a statement separator, a keyword that opens a
    /// statement, or the brace that closes a block. Section 2.3 of the
    /// specification is where the first of those comes from, and `;` is not
    /// checked for because the lexer already gives it as a newline.
    fn synchronise(&mut self) {
        loop {
            match self.peek_kind() {
                TokenKind::Eof => return,
                // Past the separator, so the next statement starts clean.
                TokenKind::Newline => {
                    self.advance();
                    return;
                }
                // Past the brace: whatever block it closed is finished with.
                TokenKind::RBrace => {
                    self.advance();
                    return;
                }
                // On the keyword, not past it, because it is the start of the
                // statement to try next.
                TokenKind::Let
                | TokenKind::Fn
                | TokenKind::If
                | TokenKind::While
                | TokenKind::For
                | TokenKind::Return
                | TokenKind::Import
                | TokenKind::Break
                | TokenKind::Continue => return,
                _ => {
                    self.advance();
                }
            }
        }
    }

    // --- Statements -------------------------------------------------------

    /// A block holds statements, and a statement can open a block, so `if`
    /// inside `if` inside `if` descends here as far as the source says to.
    fn statement(&mut self) -> Result<Stmt, MiruError> {
        let (line, column) = (self.peek().line, self.peek().column);
        self.enter(line, column)?;
        let statement = self.statement_kind(line);
        self.leave();
        statement
    }

    fn statement_kind(&mut self, line: usize) -> Result<Stmt, MiruError> {
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
                ExprKind::Identifier(_) | ExprKind::Index { .. } | ExprKind::Field { .. } => {}
                _ => {
                    return Err(MiruError::with_column(
                        expr.line,
                        expr.column,
                        "invalid assignment target (only variables, elements, and fields can be assigned to)",
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
        } else if let Some(op) = self.compound_assign_op() {
            self.advance(); // the compound operator
                            // The same targets a plain assignment accepts, and the same words
                            // when it is not one of them: `x + 1 += 2` is wrong for the reason
                            // `x + 1 = 2` is wrong, and should not read as a different mistake.
            match &expr.kind {
                ExprKind::Identifier(_) | ExprKind::Index { .. } | ExprKind::Field { .. } => {}
                _ => {
                    return Err(MiruError::with_column(
                        expr.line,
                        expr.column,
                        "invalid assignment target (only variables, elements, and fields can be assigned to)",
                    ));
                }
            }
            let value = self.expression()?;
            Ok(Stmt::new(
                StmtKind::CompoundAssign {
                    target: expr,
                    op,
                    value,
                },
                line,
            ))
        } else {
            Ok(Stmt::new(StmtKind::Expr(expr), line))
        }
    }

    /// The operator a compound assignment applies, if the next token is one.
    ///
    /// **A statement rather than an expression**, which is why this is here and
    /// not in the Pratt table. Plain assignment is a statement, so `let x = (y
    /// += 1)` stays the error it has always been rather than becoming a thing
    /// this document has to define.
    fn compound_assign_op(&self) -> Option<BinaryOp> {
        match self.peek_kind() {
            TokenKind::PlusAssign => Some(BinaryOp::Add),
            TokenKind::MinusAssign => Some(BinaryOp::Subtract),
            TokenKind::StarAssign => Some(BinaryOp::Multiply),
            TokenKind::SlashAssign => Some(BinaryOp::Divide),
            TokenKind::PercentAssign => Some(BinaryOp::Modulo),
            _ => None,
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
        // `try` binds looser than every operator, so `try a / b` covers the
        // division rather than just `a`. It lives here rather than in `unary`
        // for exactly that reason: a unary operator binds tighter than
        // multiplication, and this has to bind looser than all of them.
        //
        // It nests, so `try try f()` parses, though the inner one makes the
        // outer one unreachable.
        if self.check(&TokenKind::Try) {
            let token = self.advance().clone();
            // Charged here rather than left to `unary`: this arm calls itself,
            // so `try try try` never reaches `unary` more than once.
            self.enter(token.line, token.column)?;
            let inner = self.expression();
            self.leave();
            return Ok(Expr::new(
                ExprKind::Try(Box::new(inner?)),
                token.line,
                token.column,
            ));
        }
        self.parse_binary(1)
    }

    fn parse_binary(&mut self, min_bp: u8) -> Result<Expr, MiruError> {
        let mut left = self.unary()?;
        // Each pass wraps everything parsed so far, so the tree grows one level
        // per operator while this loop stays at a single Rust frame however far
        // it runs. Counting the recursion here would see nothing: `1 + 1 + 1
        // ...` parses happily and overflows whoever walks the result instead.
        while let Some(bp) = Parser::infix_binding_power(self.peek_kind()) {
            if bp < min_bp {
                break;
            }
            let op = self.advance();
            let right = self.parse_binary(bp + 1)?;
            left = self.checked(Parser::make_infix(&op, left, right))?;
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

    /// Every expression passes through here on its way down to `primary`, so
    /// one count covers a nested array, a nested map, a parenthesised group, a
    /// call argument, and an index, as well as the `-` and `!` chains that call
    /// this function directly.
    fn unary(&mut self) -> Result<Expr, MiruError> {
        let (line, column) = (self.peek().line, self.peek().column);
        self.enter(line, column)?;
        let expr = self.unary_operand();
        self.leave();
        expr
    }

    fn unary_operand(&mut self) -> Result<Expr, MiruError> {
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
        // A call, a field, or an index wraps what came before it, so `a[0][0]`
        // and `a.b.c` grow the tree without growing this loop. Tested the same
        // way the operator spine in `parse_binary` is, and for the same reason.
        loop {
            match self.peek_kind() {
                TokenKind::LParen => {
                    self.advance();
                    let arguments = self.parse_arguments()?;
                    let (line, column) = (expr.line, expr.column);
                    expr = self.checked(Expr::new(
                        ExprKind::Call {
                            callee: Box::new(expr),
                            arguments,
                        },
                        line,
                        column,
                    ))?;
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
                    expr = self.checked(Expr::new(
                        ExprKind::Field {
                            target: Box::new(expr),
                            name,
                        },
                        field.line,
                        field.column,
                    ))?;
                }
                TokenKind::LBracket => {
                    self.advance();
                    let index = self.expression()?;
                    self.expect(TokenKind::RBracket, "to close an index")?;
                    let (line, column) = (expr.line, expr.column);
                    expr = self.checked(Expr::new(
                        ExprKind::Index {
                            target: Box::new(expr),
                            index: Box::new(index),
                        },
                        line,
                        column,
                    ))?;
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
            TokenKind::FString(parts) => {
                self.advance();
                Ok(Expr::new(ExprKind::FString(parts), line, column))
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

/// Build the expression an `f"..."` literal stands for.
///
/// The parts are joined with `+`, and each name becomes `str(name)` so that a
/// number, a boolean, or an array renders the way `print` would show it.
///
/// **Each name keeps the position the lexer gave it**, which is the point of
/// carrying the parts this far. An unknown name inside the literal reports at
/// its own line and column, so the caret lands on the name rather than on the
/// quotation mark that opens the string.
///
/// `str` is looked up as an ordinary name. A program that defines its own
/// `str` gets its own, exactly as it does for every other builtin it shadows.
pub(crate) fn fstring_expr(parts: &[FStringPart], line: usize, column: usize) -> Expr {
    let mut joined: Option<Expr> = None;
    for part in parts {
        let piece = match part.clone() {
            FStringPart::Text(text) => Expr::new(ExprKind::Str(text), line, column),
            FStringPart::Name {
                name,
                line: name_line,
                column: name_column,
            } => Expr::new(
                ExprKind::Call {
                    callee: Box::new(Expr::new(
                        ExprKind::Identifier("str".to_string()),
                        name_line,
                        name_column,
                    )),
                    arguments: vec![Expr::new(
                        ExprKind::Identifier(name),
                        name_line,
                        name_column,
                    )],
                },
                name_line,
                name_column,
            ),
        };
        joined = Some(match joined {
            None => piece,
            Some(left) => Expr::new(
                ExprKind::Binary {
                    op: BinaryOp::Add,
                    left: Box::new(left),
                    right: Box::new(piece),
                },
                line,
                column,
            ),
        });
    }
    // `f""` is the empty string, which falls out of having no parts at all.
    joined.unwrap_or_else(|| Expr::new(ExprKind::Str(String::new()), line, column))
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

    /// The first error from tokens that should not parse.
    ///
    /// Most of these cases are about one mistake, and were written when a
    /// parse gave exactly one error. That none of them changed when the parser
    /// began reporting several is the check that recovery did not move the
    /// first one.
    fn first_error_from(tokens: Vec<Token>) -> MiruError {
        let errors = Parser::parse(tokens).expect_err("source should not parse");
        errors.into_iter().next().expect("at least one error")
    }

    /// Every error from a source that should not parse.
    fn all_errors(source: &str) -> Vec<MiruError> {
        let tokens = Lexer::tokenize(source).expect("source should tokenize");
        Parser::parse(tokens).expect_err("source should not parse")
    }

    /// Four separate mistakes report four errors, which is the whole point of
    /// issue #10: fixing them takes one run rather than four.
    #[test]
    fn every_mistake_is_reported() {
        let errors = all_errors(
            "let = 1\nlet ok = 2\nlet y = 3 +\nlet fine = 4\nprint(ok]\nlet last = 5\nfn = 9\n",
        );
        let positions: Vec<(usize, usize)> = errors.iter().map(|e| (e.line, e.column)).collect();
        assert_eq!(positions, vec![(1, 5), (3, 12), (5, 9), (7, 4)]);
    }

    /// One mistake reports exactly one error. A parser that recovers badly
    /// gives ten for one missing brace, and that is worse than the single
    /// error this replaced.
    #[test]
    fn one_mistake_reports_one_error() {
        assert_eq!(all_errors("let x = 1\nlet = 2\nlet z = 3\n").len(), 1);
        assert_eq!(all_errors("print(1))\n").len(), 1);
    }

    /// **The cascade case.** An unclosed bracket makes the lexer suppress every
    /// newline after it, so no later statement can be terminated and each one
    /// fails with the same complaint. The parser cannot recover information the
    /// lexer already dropped; what it can do is notice it is not recovering.
    #[test]
    fn an_unclosed_bracket_does_not_bury_its_own_error() {
        let mut source = String::from("let a = (\n");
        for i in 0..400 {
            source.push_str(&format!("let v{i} = {i}\n"));
        }
        let errors = all_errors(&source);
        assert!(
            errors.len() <= Parser::MAX_CONSECUTIVE,
            "one missing bracket gave {} errors",
            errors.len()
        );
        // The first one is the real one, and it points at the opening line.
        assert_eq!(errors[0].line, 2);
    }

    /// The counter test from issue #10, which calls it the condition most
    /// likely to be missed: an early error followed by hundreds of ordinary
    /// statements must not report `the program is nested too deeply` somewhere
    /// nothing is nested.
    #[test]
    fn recovery_leaves_the_nesting_counter_alone() {
        let mut source = String::from("let = 1\n");
        for i in 0..400 {
            source.push_str(&format!("let v{i} = {i}\n"));
        }
        let errors = all_errors(&source);
        assert_eq!(errors.len(), 1, "only the real mistake");
        assert!(
            !errors
                .iter()
                .any(|e| e.message.contains("nested too deeply")),
            "the counter drifted: {errors:?}"
        );
    }

    /// Past the nesting limit the parse stops rather than recovering, because
    /// that is the one error raised before its own `leave` runs. Recovering
    /// from it would leave the count high for the rest of the file.
    /// On its own thread, because a libtest thread gets 2 MiB and reaching the
    /// nesting limit needs more than that. `miru` starts a 64 MiB thread for
    /// the same reason, and the deep cases in `tests/golden.rs` say so too.
    /// Without it this test overflows before it reaches the limit and measures
    /// libtest rather than the parser.
    #[test]
    fn the_nesting_limit_stops_the_parse() {
        let errors = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                // A second, separate mistake after the deep one. It is not
                // reported, because the parse stopped: that is what the guard
                // does, and asserting the count alone would not show it.
                let source = format!(
                    "let deep = {}{}\nlet ok = 1\nlet = 2\n",
                    "[".repeat(Parser::MAX_NESTING + 5),
                    "]".repeat(Parser::MAX_NESTING + 5)
                );
                all_errors(&source)
            })
            .expect("the operating system can start a thread")
            .join()
            .expect("the test body did not panic");
        assert_eq!(
            errors.len(),
            1,
            "the parse should stop rather than recover: {errors:?}"
        );
        assert!(errors[0].message.contains("nested too deeply"));
    }

    /// A wall of errors is as unhelpful as one, so the count is capped. Each
    /// mistake here is separated by a working statement, so nothing else stops
    /// the parse first.
    #[test]
    fn the_number_of_errors_is_capped() {
        let mut source = String::new();
        for i in 0..40 {
            source.push_str(&format!("let = {i}\nlet ok{i} = {i}\n"));
        }
        assert_eq!(all_errors(&source).len(), Parser::MAX_ERRORS);
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
        let err = first_error_from(tokens);
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
        let err = first_error_from(tokens);
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
        let err = first_error_from(tokens);
        assert!(err.message.contains("break outside of a loop"));
    }

    #[test]
    fn break_cannot_target_a_loop_outside_a_function() {
        let tokens =
            Lexer::tokenize("for i in [1] {\n  fn f() {\n    break\n  }\n}").expect("lexes");
        let err = first_error_from(tokens);
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
