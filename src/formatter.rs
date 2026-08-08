//! The source formatter behind `miru fmt`: re-prints a parsed program in a
//! single canonical style.
//!
//! The formatter walks the AST rather than the original text, so it decides
//! every space, indent, and line break itself. Two-space indentation, one
//! statement per line, and inline array, map, and anonymous-function literals
//! give a predictable, idempotent result: formatting already-formatted source
//! leaves it unchanged.
//!
//! Comments are not part of the AST, so they are carried alongside it as
//! [`Trivia`] (gathered by [`crate::lexer::Lexer::tokenize_with_trivia`]) and
//! reattached here. An own-line comment becomes a leading comment on the
//! statement that follows it; a comment sharing a line with code becomes a
//! trailing comment on that statement. A single blank line between sections is
//! preserved.

use std::collections::HashSet;

use crate::ast::{BinaryOp, Expr, ExprKind, LogicalOp, Stmt, StmtKind, UnaryOp};

/// A single line comment, remembered so the formatter can put it back.
#[derive(Debug, Clone)]
pub struct Comment {
    /// The 1-based line the comment sits on.
    pub line: usize,
    /// True when the comment is alone on its line (a leading comment), false
    /// when it trails code on the same line.
    pub own_line: bool,
    /// The comment's text, without the leading `//` and surrounding whitespace.
    pub text: String,
}

/// Everything the parser drops but the formatter needs: the comments, and the
/// set of lines that had a blank line immediately above them.
#[derive(Debug, Clone, Default)]
pub struct Trivia {
    pub comments: Vec<Comment>,
    pub blank_before: HashSet<usize>,
}

/// Format a parsed program back into canonical source text. The result always
/// ends with a single newline, unless the program is empty.
pub fn format_program(program: &[Stmt], trivia: &Trivia) -> String {
    let mut printer = Printer {
        out: String::new(),
        comments: &trivia.comments,
        idx: 0,
        blank_before: &trivia.blank_before,
        blanked: HashSet::new(),
        last_blank: false,
    };
    printer.print_stmts(program, 0);
    printer.flush_remaining(0);
    printer.out
}

/// Carries the growing output and the position within the comment list.
struct Printer<'a> {
    out: String,
    comments: &'a [Comment],
    idx: usize,
    blank_before: &'a HashSet<usize>,
    /// Lines whose leading blank has already been emitted, so a line shared by
    /// several statements (such as `fn f() { return 1 }`) blanks only once.
    blanked: HashSet<usize>,
    last_blank: bool,
}

impl Printer<'_> {
    /// Print a run of statements, each preceded by any leading comments.
    fn print_stmts(&mut self, stmts: &[Stmt], indent: usize) {
        for stmt in stmts {
            self.emit_leading(stmt.line, indent);
            self.print_stmt(stmt, indent);
        }
    }

    fn print_stmt(&mut self, stmt: &Stmt, indent: usize) {
        self.maybe_blank(stmt.line);
        match &stmt.kind {
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let header = format!("if {} {{", fmt_expr(condition));
                let header = self.attach_trailing(stmt.line, header);
                self.push_line(indent, header);
                self.print_stmts(then_branch, indent + 1);
                self.print_else(else_branch, indent);
            }
            StmtKind::While { condition, body } => {
                let header = format!("while {} {{", fmt_expr(condition));
                let header = self.attach_trailing(stmt.line, header);
                self.push_line(indent, header);
                self.print_stmts(body, indent + 1);
                self.push_line(indent, "}".to_string());
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                let header = format!("for {name} in {} {{", fmt_expr(iterable));
                let header = self.attach_trailing(stmt.line, header);
                self.push_line(indent, header);
                self.print_stmts(body, indent + 1);
                self.push_line(indent, "}".to_string());
            }
            StmtKind::Function { name, params, body } => {
                let header = format!("fn {name}({}) {{", params.join(", "));
                let header = self.attach_trailing(stmt.line, header);
                self.push_line(indent, header);
                self.print_stmts(body, indent + 1);
                self.push_line(indent, "}".to_string());
            }
            // Every other statement fits on a single line.
            _ => {
                let text = stmt_inline(stmt);
                let text = self.attach_trailing(stmt.line, text);
                self.push_line(indent, text);
            }
        }
    }

    /// Print the `else` arm of an `if`, folding `else { if ... }` into the
    /// canonical `else if ...` chain.
    fn print_else(&mut self, else_branch: &Option<Vec<Stmt>>, indent: usize) {
        match else_branch {
            None => self.push_line(indent, "}".to_string()),
            Some(branch) => match single_if(branch) {
                Some(nested) => {
                    if let StmtKind::If {
                        condition,
                        then_branch,
                        else_branch,
                    } = &nested.kind
                    {
                        let header = format!("}} else if {} {{", fmt_expr(condition));
                        let header = self.attach_trailing(nested.line, header);
                        self.push_line(indent, header);
                        self.print_stmts(then_branch, indent + 1);
                        self.print_else(else_branch, indent);
                    }
                }
                None => {
                    self.push_line(indent, "} else {".to_string());
                    self.print_stmts(branch, indent + 1);
                    self.push_line(indent, "}".to_string());
                }
            },
        }
    }

    /// Emit the leading (own-line) comments that come before `before_line`.
    fn emit_leading(&mut self, before_line: usize, indent: usize) {
        while self.idx < self.comments.len() {
            let comment = &self.comments[self.idx];
            if !comment.own_line || comment.line >= before_line {
                break;
            }
            let line = comment.line;
            let text = render_comment(&comment.text);
            self.idx += 1;
            self.maybe_blank(line);
            self.push_line(indent, text);
        }
    }

    /// If a comment trails code on `line`, append it to `text` and consume it.
    fn attach_trailing(&mut self, line: usize, text: String) -> String {
        if let Some(comment) = self.comments.get(self.idx) {
            if !comment.own_line && comment.line == line {
                let rendered = render_comment(&comment.text);
                self.idx += 1;
                return format!("{text}  {rendered}");
            }
        }
        text
    }

    /// Emit any comments left over once every statement has been printed, so no
    /// comment is ever silently dropped.
    fn flush_remaining(&mut self, indent: usize) {
        while self.idx < self.comments.len() {
            let line = self.comments[self.idx].line;
            let text = render_comment(&self.comments[self.idx].text);
            self.idx += 1;
            self.maybe_blank(line);
            self.push_line(indent, text);
        }
    }

    /// Emit a single blank line if `line` had one above it in the source and it
    /// has not been blanked already.
    fn maybe_blank(&mut self, line: usize) {
        if self.blank_before.contains(&line) && self.blanked.insert(line) {
            self.push_blank();
        }
    }

    fn push_blank(&mut self) {
        if self.out.is_empty() || self.last_blank {
            return;
        }
        self.out.push('\n');
        self.last_blank = true;
    }

    fn push_line(&mut self, indent: usize, text: String) {
        for _ in 0..indent {
            self.out.push_str("  ");
        }
        self.out.push_str(&text);
        self.out.push('\n');
        self.last_blank = false;
    }
}

/// The single statement in `branch`, when it is exactly one `if`. Used to detect
/// an `else if` chain.
fn single_if(branch: &[Stmt]) -> Option<&Stmt> {
    match branch {
        [only] if matches!(only.kind, StmtKind::If { .. }) => Some(only),
        _ => None,
    }
}

/// Normalize a comment to `// text`, or a bare `//` when it has no text.
fn render_comment(text: &str) -> String {
    let content = text.trim();
    if content.is_empty() {
        "//".to_string()
    } else {
        format!("// {content}")
    }
}

/// Render a statement as a single line. Block statements are rendered inline,
/// which is what makes anonymous function bodies printable inside expressions.
fn stmt_inline(stmt: &Stmt) -> String {
    match &stmt.kind {
        StmtKind::Import {
            spec,
            alias,
            column: _,
        } => format!("import {} as {alias}", fmt_string(spec)),
        StmtKind::Let { name, value } => format!("let {name} = {}", fmt_expr(value)),
        StmtKind::Assign { target, value } => {
            format!("{} = {}", fmt_expr(target), fmt_expr(value))
        }
        StmtKind::CompoundAssign { target, op, value } => {
            // Printed as written rather than expanded to the long form. The
            // formatter reprints a program in canonical style, and `x += 1` is
            // the canonical way to write `x += 1`.
            format!(
                "{} {}= {}",
                fmt_expr(target),
                binary_symbol(*op),
                fmt_expr(value)
            )
        }
        StmtKind::Expr(expr) => fmt_expr(expr),
        StmtKind::Return(None) => "return".to_string(),
        StmtKind::Return(Some(expr)) => format!("return {}", fmt_expr(expr)),
        StmtKind::Break => "break".to_string(),
        StmtKind::Continue => "continue".to_string(),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let mut text = format!("if {} {}", fmt_expr(condition), block_inline(then_branch));
            text.push_str(&else_inline(else_branch));
            text
        }
        StmtKind::While { condition, body } => {
            format!("while {} {}", fmt_expr(condition), block_inline(body))
        }
        StmtKind::For {
            name,
            iterable,
            body,
        } => format!(
            "for {name} in {} {}",
            fmt_expr(iterable),
            block_inline(body)
        ),
        StmtKind::Function { name, params, body } => {
            format!("fn {name}({}) {}", params.join(", "), block_inline(body))
        }
    }
}

fn else_inline(else_branch: &Option<Vec<Stmt>>) -> String {
    match else_branch {
        None => String::new(),
        Some(branch) => match single_if(branch) {
            Some(nested) => format!(" else {}", stmt_inline(nested)),
            None => format!(" else {}", block_inline(branch)),
        },
    }
}

/// Render a block inline, as `{}` when empty or `{ a; b }` otherwise.
fn block_inline(stmts: &[Stmt]) -> String {
    if stmts.is_empty() {
        "{}".to_string()
    } else {
        let parts: Vec<String> = stmts.iter().map(stmt_inline).collect();
        format!("{{ {} }}", parts.join("; "))
    }
}

/// The binding power of an expression, matching the parser's table. Larger binds
/// tighter. Used to decide when a subexpression needs parentheses.
fn precedence(expr: &Expr) -> u8 {
    match &expr.kind {
        ExprKind::Logical {
            op: LogicalOp::Or, ..
        } => 1,
        ExprKind::Logical {
            op: LogicalOp::And, ..
        } => 2,
        ExprKind::Binary { op, .. } => match op {
            BinaryOp::Equal | BinaryOp::NotEqual => 3,
            BinaryOp::Less | BinaryOp::Greater | BinaryOp::LessEqual | BinaryOp::GreaterEqual => 4,
            BinaryOp::Add | BinaryOp::Subtract => 5,
            BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Modulo => 6,
        },
        // Looser than every operator, `or` included, because `try` takes the
        // whole expression after it. Left at the wildcard's 9 the printer would
        // drop the parentheses in `(try f()) + 1` and reprint it as
        // `try f() + 1`, which parses back as a different tree.
        ExprKind::Try(_) => 0,
        ExprKind::Unary { .. } => 7,
        // Field binds like the other postfix forms. The arm below ends in a
        // wildcard, so a variant missing here silently takes 9 and the printer
        // then drops parentheses it needed. Nothing fails; the output is just
        // wrong.
        ExprKind::Call { .. } | ExprKind::Index { .. } | ExprKind::Field { .. } => 8,
        _ => 9,
    }
}

/// Render a subexpression, wrapping it in parentheses when its binding power is
/// below `min` (so the reprinted form parses back to the same tree).
fn fmt_operand(expr: &Expr, min: u8) -> String {
    let text = fmt_expr(expr);
    if precedence(expr) < min {
        format!("({text})")
    } else {
        text
    }
}

/// Render an expression on a single line.
fn fmt_expr(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Int(n) => n.to_string(),
        ExprKind::Float(f) => fmt_float(*f),
        ExprKind::Str(s) => fmt_string(s),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Nil => "nil".to_string(),
        ExprKind::Identifier(name) => name.clone(),
        ExprKind::Array(elements) => {
            let parts: Vec<String> = elements.iter().map(fmt_expr).collect();
            format!("[{}]", parts.join(", "))
        }
        ExprKind::Map(entries) => {
            if entries.is_empty() {
                "{}".to_string()
            } else {
                let parts: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", fmt_expr(key), fmt_expr(value)))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
        }
        ExprKind::Field { target, name } => {
            format!("{}.{name}", fmt_operand(target, 8))
        }
        ExprKind::Index { target, index } => {
            format!("{}[{}]", fmt_operand(target, 8), fmt_expr(index))
        }
        ExprKind::Try(inner) => format!("try {}", fmt_expr(inner)),
        ExprKind::Unary { op, operand } => {
            let symbol = match op {
                UnaryOp::Negate => "-",
                UnaryOp::Not => "!",
            };
            format!("{symbol}{}", fmt_operand(operand, 7))
        }
        ExprKind::Binary { op, left, right } => {
            let power = precedence(expr);
            format!(
                "{} {} {}",
                fmt_operand(left, power),
                binary_symbol(*op),
                fmt_operand(right, power + 1)
            )
        }
        ExprKind::Logical { op, left, right } => {
            let power = precedence(expr);
            let symbol = match op {
                LogicalOp::And => "&&",
                LogicalOp::Or => "||",
            };
            format!(
                "{} {symbol} {}",
                fmt_operand(left, power),
                fmt_operand(right, power + 1)
            )
        }
        ExprKind::Call { callee, arguments } => {
            let args: Vec<String> = arguments.iter().map(fmt_expr).collect();
            format!("{}({})", fmt_operand(callee, 8), args.join(", "))
        }
        ExprKind::Function { params, body } => {
            format!("fn({}) {}", params.join(", "), block_inline(body))
        }
    }
}

fn binary_symbol(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Subtract => "-",
        BinaryOp::Multiply => "*",
        BinaryOp::Divide => "/",
        BinaryOp::Modulo => "%",
        BinaryOp::Equal => "==",
        BinaryOp::NotEqual => "!=",
        BinaryOp::Less => "<",
        BinaryOp::Greater => ">",
        BinaryOp::LessEqual => "<=",
        BinaryOp::GreaterEqual => ">=",
    }
}

/// Render a float literal so it always carries a decimal point and reparses to
/// the same value, matching how the runtime prints floats.
fn fmt_float(f: f64) -> String {
    let text = f.to_string();
    if text.contains(['.', 'e', 'E']) {
        text
    } else {
        format!("{text}.0")
    }
}

/// Render a string literal with quotes and the escapes the lexer understands.
///
/// The same function that decides what a program prints for a string inside an
/// array, because the two answers have to agree. They were two copies of one
/// list until 1.4, and the list was wrong in both.
fn fmt_string(s: &str) -> String {
    crate::value::quoted_string(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(source: &str) -> Vec<Stmt> {
        crate::parse_program(source).expect("source should parse")
    }

    /// Format comment-free source through a full parse.
    fn fmt(source: &str) -> String {
        format_program(&program(source), &Trivia::default())
    }

    #[test]
    fn try_reprints_with_the_parentheses_it_needs() {
        // Plain, and the reason `try` needs a precedence of its own: it takes
        // the whole expression after it, so it binds looser than every
        // operator including `or`.
        assert_eq!(fmt("let r = try 1/0\n"), "let r = try 1 / 0\n");

        // The case the wildcard would have broken. `try` reached as an operand
        // has to keep its parentheses, or this reprints as `try f() + 1` and
        // parses back with the addition inside the guard instead of outside
        // it: a different program that still runs.
        assert_eq!(fmt("let n = (try f()) + 1\n"), "let n = (try f()) + 1\n");
        assert_eq!(fmt("let b = (try f()) || 2\n"), "let b = (try f()) || 2\n");

        // And formatting is idempotent, which is what says the parentheses it
        // adds are ones it also accepts.
        let once = fmt("let n = (try f()) + 1\n");
        assert_eq!(fmt(&once), once);
    }

    #[test]
    fn a_control_character_is_spelled_rather_than_written() {
        // The defect this closes: `miru fmt` used to write the character
        // itself, so formatting a file that held one put a byte into the
        // source that no editor shows.
        assert_eq!(fmt("let s = \"\\u{7}\"\n"), "let s = \"\\u{7}\"\n");
        assert_eq!(fmt("let s = \"\\u{1B}\"\n"), "let s = \"\\u{1B}\"\n");
        // `\0` keeps its own spelling rather than becoming `\u{0}`.
        assert_eq!(fmt("let s = \"\\0\"\n"), "let s = \"\\0\"\n");

        // Which makes the formatter a fixed point on its own output, where
        // before it produced a file it could not have written twice the same.
        let once = fmt("let s = \"\\u{1B}\"\n");
        assert_eq!(fmt(&once), once);
    }

    #[test]
    fn a_character_that_is_not_ascii_is_written_as_itself() {
        // `\u{...}` is a way to write a character, not a second kind of string.
        // A token carries its value and not the text it came from, so the
        // formatter has nothing to write the escape back from, and writes the
        // character. What has to survive is the value, and it does.
        assert_eq!(fmt("let e = \"\\u{1F600}\"\n"), "let e = \"\u{1F600}\"\n");

        // The result formats to itself, so `miru fmt` is still a fixed point on
        // its own output. This is the same normalizing the formatter already
        // does to a number, where `1.50` comes back as `1.5`.
        let once = fmt("let e = \"\\u{1F600}\"\n");
        assert_eq!(fmt(&once), once);

        // The escapes the formatter does write are unaffected.
        assert_eq!(fmt("let s = \"a\\nb\"\n"), "let s = \"a\\nb\"\n");
    }

    #[test]
    fn prints_arithmetic_with_minimal_parentheses() {
        assert_eq!(fmt("1 + 2 * 3"), "1 + 2 * 3\n");
        assert_eq!(fmt("(1 + 2) * 3"), "(1 + 2) * 3\n");
    }

    #[test]
    fn keeps_parentheses_that_matter_for_associativity() {
        assert_eq!(fmt("1 - (2 - 3)"), "1 - (2 - 3)\n");
        assert_eq!(fmt("1 - 2 - 3"), "1 - 2 - 3\n");
    }

    #[test]
    fn parenthesizes_lower_precedence_operands() {
        assert_eq!(fmt("-(a + b)"), "-(a + b)\n");
        assert_eq!(fmt("!(a && b)"), "!(a && b)\n");
        assert_eq!(fmt("(a + b)[0]"), "(a + b)[0]\n");
    }

    #[test]
    fn field_access_keeps_only_the_parentheses_it_needs() {
        // A field binds like the other postfix forms, so a chain needs no
        // parentheses and a lower-precedence target still does.
        assert_eq!(fmt("(a.b).c"), "a.b.c\n");
        assert_eq!(fmt("a.b()"), "a.b()\n");
        assert_eq!(fmt("a[0].b"), "a[0].b\n");
        assert_eq!(fmt("a.b[0]"), "a.b[0]\n");
        assert_eq!(fmt("(a + b).c"), "(a + b).c\n");
        assert_eq!(fmt("(-a).b"), "(-a).b\n");
        assert_eq!(fmt("-a.b"), "-a.b\n");
    }

    #[test]
    fn an_import_keeps_its_quotes_and_one_space_either_side() {
        assert_eq!(
            fmt("import  \"./a.miru\"  as   a"),
            "import \"./a.miru\" as a\n"
        );
    }

    #[test]
    fn normalizes_spacing_and_literals() {
        assert_eq!(fmt("let  x=[1,2 , 3]"), "let x = [1, 2, 3]\n");
        assert_eq!(
            fmt("let m = {\"b\":2,\"a\":1}"),
            "let m = {\"b\": 2, \"a\": 1}\n"
        );
        assert_eq!(fmt("print( 1.0 , \"hi\" )"), "print(1.0, \"hi\")\n");
    }

    #[test]
    fn indents_blocks_with_two_spaces() {
        let source = "fn add(a,b){return a+b}";
        assert_eq!(fmt(source), "fn add(a, b) {\n  return a + b\n}\n");
    }

    #[test]
    fn folds_else_if_chains() {
        let source = "if a {\n1\n} else {\nif b {\n2\n} else {\n3\n}\n}";
        assert_eq!(
            fmt(source),
            "if a {\n  1\n} else if b {\n  2\n} else {\n  3\n}\n"
        );
    }

    #[test]
    fn prints_anonymous_functions_inline() {
        assert_eq!(
            fmt("map(xs, fn(x){return x*2})"),
            "map(xs, fn(x) { return x * 2 })\n"
        );
    }

    #[test]
    fn is_idempotent_on_a_range_of_constructs() {
        let sources = [
            "let x = 1 + 2 * 3",
            "fn f(n) {\n  if n < 2 {\n    return n\n  }\n  return f(n - 1)\n}",
            "for i in range(10) {\n  print(i)\n}",
            "while a && b || c {\n  x = x + 1\n}",
            "let data = [1, 2, [3, 4], {\"k\": v}]",
            "map(xs, fn(x) { return x * 2 })",
        ];
        for source in sources {
            let once = fmt(source);
            assert_eq!(fmt(&once), once, "not idempotent for: {source}");
        }
    }

    #[test]
    fn round_trips_through_the_parser() {
        let sources = [
            "let x = -(a + b) * c",
            "if p {\n  q\n} else if r {\n  s\n}",
            "print(a, b, fn(x) { return x + 1 })",
        ];
        for source in sources {
            let formatted = fmt(source);
            assert_eq!(
                program(&formatted),
                program(source),
                "reprint changed the tree for: {source}"
            );
        }
    }

    #[test]
    fn reattaches_a_leading_comment() {
        // "let y = 2" is on line 3 because of the blank second line.
        let stmts = program("let x = 1\n\nlet y = 2");
        let trivia = Trivia {
            comments: vec![Comment {
                line: 2,
                own_line: true,
                text: " a note".to_string(),
            }],
            blank_before: HashSet::new(),
        };
        assert_eq!(
            format_program(&stmts, &trivia),
            "let x = 1\n// a note\nlet y = 2\n"
        );
    }

    #[test]
    fn reattaches_a_trailing_comment() {
        let stmts = program("let x = 1\nlet y = 2");
        let trivia = Trivia {
            comments: vec![Comment {
                line: 1,
                own_line: false,
                text: " trailing".to_string(),
            }],
            blank_before: HashSet::new(),
        };
        assert_eq!(
            format_program(&stmts, &trivia),
            "let x = 1  // trailing\nlet y = 2\n"
        );
    }

    #[test]
    fn preserves_a_single_blank_line() {
        let stmts = program("let x = 1\n\nlet y = 2");
        let trivia = Trivia {
            comments: Vec::new(),
            blank_before: HashSet::from([3]),
        };
        assert_eq!(format_program(&stmts, &trivia), "let x = 1\n\nlet y = 2\n");
    }

    #[test]
    fn a_blank_above_a_shared_line_blanks_only_once() {
        // The function header and its body statement are both on line 3, so the
        // blank above line 3 must not repeat inside the body.
        let stmts = program("let x = 1\n\nfn f() { return 2 }");
        let trivia = Trivia {
            comments: Vec::new(),
            blank_before: HashSet::from([3]),
        };
        assert_eq!(
            format_program(&stmts, &trivia),
            "let x = 1\n\nfn f() {\n  return 2\n}\n"
        );
    }

    #[test]
    fn indents_a_comment_inside_a_block() {
        // The comment on line 2 leads the print on line 3, inside the block.
        let stmts = program("if c {\n\n  print(1)\n}");
        let trivia = Trivia {
            comments: vec![Comment {
                line: 2,
                own_line: true,
                text: " inside".to_string(),
            }],
            blank_before: HashSet::new(),
        };
        assert_eq!(
            format_program(&stmts, &trivia),
            "if c {\n  // inside\n  print(1)\n}\n"
        );
    }
}
