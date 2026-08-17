//! The abstract syntax tree (AST) that the parser produces and the compiler
//! turns into bytecode. Nodes carry the source position they begin at, which
//! the compiler records so runtime errors can point at the right place.

/// A statement, tagged with its 1-based starting line.
#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}

impl Stmt {
    pub fn new(kind: StmtKind, line: usize) -> Stmt {
        Stmt { kind, line }
    }
}

/// What a binding binds to: a name, or a bracketed list that takes an array
/// apart.
///
/// `let` and `for` both bind, so both take one of these, and nesting falls out
/// of the recursion rather than being a case of its own. **The parser counts
/// its own recursion while reading one**, because the compiler and the
/// formatter walk it by recursion too, and source nested past the Rust stack
/// aborts the process rather than reporting.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Name(String),
    /// `[a, b]`, and `[]` for an array that must be empty.
    Array(Vec<Pattern>),
}

impl Pattern {
    /// The names this pattern binds, in the order it binds them.
    ///
    /// A name may appear twice. Nothing here refuses that: `let [x, x] = pair`
    /// binds `x` twice in one statement, and the shadowing rule that already
    /// governs `let x = 1` followed by `let x = 2` says the later one wins.
    pub fn names(&self) -> Vec<&str> {
        let mut found = Vec::new();
        self.collect_names(&mut found);
        found
    }

    fn collect_names<'a>(&'a self, found: &mut Vec<&'a str>) {
        match self {
            Pattern::Name(name) => found.push(name),
            Pattern::Array(items) => {
                for item in items {
                    item.collect_names(found);
                }
            }
        }
    }
}

/// One named parameter, with the expression that fills it when a call does not.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    /// `None` for a parameter a call must supply.
    ///
    /// **The expression is kept rather than a value**, because a default is
    /// evaluated at each call that omits it rather than once at definition.
    /// Issue #45 settled that: it is what `fn f(t = now())` has to mean for the
    /// time to belong to the call, and it is what keeps `fn f(a = [])` from
    /// sharing one array between every call, which is the trap Python is known
    /// for.
    pub default: Option<Expr>,
}

/// A function's whole parameter list.
///
/// **The rest parameter is its own field rather than an entry in the list**,
/// because the rules that make a call matchable are rules about the shape:
/// nothing required may follow something defaulted, and `...rest` is last. One
/// flat list with markers would let the parser build a shape the compiler then
/// has to refuse, and the refusal belongs where the syntax is read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Params {
    /// The named parameters, required ones first, then defaulted ones.
    pub named: Vec<Param>,
    /// The name of a `...rest` parameter, which is an ordinary array inside the
    /// body holding whatever arguments came past the named ones.
    pub rest: Option<String>,
}

impl Params {
    /// How many arguments a call must supply.
    pub fn required(&self) -> usize {
        self.named.iter().filter(|p| p.default.is_none()).count()
    }

    /// Every name this list binds, in the order their slots are laid out.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.named
            .iter()
            .map(|p| p.name.as_str())
            .chain(self.rest.as_deref())
    }
}

/// One arm of a `match`.
///
/// `B` is what the arm holds: `Vec<Stmt>` for a `match` used as a statement and
/// `Expr` for one used as a value. That mirrors `if`, which has had the same
/// two forms since 1.11 for the same reason: an arm that declares a local
/// leaves it underneath the arm's value, and restricting the value form to one
/// expression is what avoids needing an instruction to reach past it.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm<B> {
    /// The values this arm takes, compared with `==`. **Empty means `else`**,
    /// the arm that always matches.
    ///
    /// Several cases share one arm because the code this was filed for wants
    /// that: `pressed == "q" || pressed == "ctrl+c" || pressed == "escape"` is
    /// one decision written three times.
    pub cases: Vec<Expr>,
    /// An extra test, run only when one of the cases matched.
    ///
    /// Not optional to the feature, whatever the issue first assumed. Every
    /// chain #48 pointed at carries an extra predicate (`&& fits(..)`,
    /// `&& facing_y == 0`), so a `match` on the value alone fixes none of them.
    pub guard: Option<Expr>,
    pub body: B,
}

/// The different kinds of statements in the language.
#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// `import "./math.miru" as math`. Binds the imported file's globals as a
    /// map under `alias`, reached with field access.
    Import {
        /// The path as written, resolved relative to the importing file.
        spec: String,
        alias: String,
        /// Where the path literal sits, so a resolution failure points at it
        /// rather than at the start of the line.
        column: usize,
    },
    /// `let name = value`, or `let [a, b] = value` to take an array apart.
    Let { pattern: Pattern, value: Expr },
    /// `target = value`, where `target` is an identifier or an index expression.
    Assign { target: Expr, value: Expr },
    /// `target op= value`, such as `x += 1`.
    ///
    /// **Kept as its own statement rather than rewritten into
    /// `target = target op value`, because that rewrite evaluates the target
    /// twice.** `a[next()] += 1` would call `next` once to read and again to
    /// store, leaving the sugar with a meaning the long form does not have. The
    /// compiler evaluates the parts of the target once and works from copies.
    CompoundAssign {
        target: Expr,
        op: BinaryOp,
        value: Expr,
    },
    /// A bare expression evaluated for its side effects, such as `print(x)`.
    Expr(Expr),
    /// `return` with an optional value.
    Return(Option<Expr>),
    /// `break` out of the nearest enclosing loop.
    Break,
    /// `continue` to the next iteration of the nearest enclosing loop.
    Continue,
    /// `if condition { .. } else { .. }`; the else branch is optional.
    If {
        condition: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    /// `while condition { .. }`
    While { condition: Expr, body: Vec<Stmt> },
    /// `for name in iterable { .. }`, or `for key, value in map { .. }`.
    ///
    /// `value` is the second loop variable. When it is present the iterable is
    /// walked as pairs: a map gives its key and value, an array gives its index
    /// and element.
    ///
    /// Both positions are patterns, so `for [x, y] in cells` takes each element
    /// apart and `for i, [x, y] in cells` does both at once. Allowing a pattern
    /// in one position and not the other would be a carve-out with nothing
    /// behind it.
    For {
        name: Pattern,
        value: Option<Pattern>,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    /// A named function declaration: `fn name(params) { .. }`.
    Function {
        name: String,
        params: Params,
        body: Vec<Stmt>,
    },
    /// `match subject { case { .. } else { .. } }`, used for its effects.
    ///
    /// The value form is [`ExprKind::Match`]. Which one is built follows which
    /// position it was written in, exactly as it does for `if`.
    Match {
        subject: Expr,
        arms: Vec<MatchArm<Vec<Stmt>>>,
        /// Where the `match` keyword sits, so a value that no arm takes can
        /// point a caret at the construct that refused it.
        column: usize,
    },
}

/// An expression, tagged with its 1-based starting line and column so runtime
/// errors can point a caret at the exact expression that failed.
#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: usize,
    pub column: usize,
    /// How many levels this expression holds, counting itself. A literal is 1.
    ///
    /// The compiler, the formatter, and the code that releases the tree all
    /// walk it by recursion, so a tall tree overflows the Rust stack and aborts
    /// the process. The parser refuses to build one, and this is the figure it
    /// tests. It is kept on the node because it cannot be recovered later
    /// without the recursive walk that it exists to make safe.
    pub height: usize,
}

impl Expr {
    pub fn new(kind: ExprKind, line: usize, column: usize) -> Expr {
        let height = kind.height();
        Expr {
            kind,
            line,
            column,
            height,
        }
    }
}

/// Two expressions are equal when their kinds match; positions are ignored so
/// tests can compare tree shape without tracking every line and column. The
/// height follows from the kind, so it needs no separate test.
impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

fn tallest(exprs: &[Expr]) -> usize {
    exprs.iter().map(|expr| expr.height).max().unwrap_or(0)
}

impl ExprKind {
    /// One more than the tallest child.
    ///
    /// Each child already carries its own height, so this reads one level and
    /// not the whole tree. Every node is built exactly once, through
    /// [`Expr::new`], which makes the total cost of keeping the figure linear
    /// in the size of the program.
    ///
    /// A function literal counts as a leaf. Its body holds statements, not
    /// expressions, and the parser limits how deeply statements nest by
    /// counting its own recursion.
    fn height(&self) -> usize {
        let below = match self {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::Nil
            | ExprKind::Identifier(_)
            // A leaf: its parts hold text and names, not expressions.
            | ExprKind::FString(_)
            | ExprKind::Function { .. } => 0,
            ExprKind::Array(items) => tallest(items),
            ExprKind::Map(pairs) => pairs
                .iter()
                .map(|(key, value)| key.height.max(value.height))
                .max()
                .unwrap_or(0),
            ExprKind::Index { target, index } => target.height.max(index.height),
            ExprKind::Field { target, .. } => target.height,
            ExprKind::Unary { operand, .. } => operand.height,
            ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
                left.height.max(right.height)
            }
            ExprKind::Call { callee, arguments } => callee.height.max(tallest(arguments)),
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => condition.height.max(then_value.height).max(else_value.height),
            ExprKind::Try(inner) => inner.height,
            ExprKind::Match { subject, arms } => arms
                .iter()
                .map(|arm| {
                    tallest(&arm.cases)
                        .max(arm.guard.as_ref().map_or(0, |g| g.height))
                        .max(arm.body.height)
                })
                .max()
                .unwrap_or(0)
                .max(subject.height),
        };
        below + 1
    }
}

/// The different kinds of expressions in the language.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// An `f"..."` literal, kept whole rather than expanded here.
    ///
    /// **The expansion belongs to the compiler, not the parser.** `miru fmt`
    /// reprints the AST, so a parser that turned this into
    /// `"a" + str(n)` would rewrite the author's f-string away every time the
    /// formatter ran. Keeping the parts means the formatter can print back what
    /// was written.
    FString(Vec<crate::token::FStringPart>),
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Nil,
    Identifier(String),
    Array(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    /// `target.name`. Distinct from [`ExprKind::Index`] with a string index
    /// because a field that is not there is an error, where a missing map key
    /// reads as `nil`.
    Field {
        target: Box<Expr>,
        name: String,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Logical {
        op: LogicalOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
    /// `if condition { a } else { b }`, used where a value is wanted.
    ///
    /// **An arm holds one expression, not a block of statements.** The general
    /// form is addable later, because it is an error now; the reverse is not
    /// true. It is left out because an arm that declares a local leaves that
    /// local underneath the arm's value on the stack, and removing it from
    /// there needs either a new instruction or a renumbering pass, neither of
    /// which buys anything a reader of `if c { a } else { b }` wanted. The
    /// f-string took the same shape in 1.9: a name between the braces, with an
    /// expression left to a release that needs one.
    ///
    /// The `else` is not optional here, unlike [`StmtKind::If`]. An `if`
    /// without one has no value to give when the condition is false, and `nil`
    /// would be inventing one.
    If {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    /// `match subject { case { value } else { value } }`, used where a value
    /// is wanted. The value is the arm's.
    ///
    /// **An arm holds one expression, as an `if` arm does**, for the reason
    /// recorded on [`ExprKind::If`]: a local declared in an arm would sit
    /// underneath the arm's value with no instruction able to reach it. The
    /// general form stays addable, because it is an error now.
    Match {
        subject: Box<Expr>,
        arms: Vec<MatchArm<Expr>>,
    },
    /// `try expr`. Evaluates the expression and yields its value, or, if
    /// evaluating it fails at any depth, the error itself as a value.
    ///
    /// Takes the whole expression that follows rather than binding tightly like
    /// a unary operator, so `try a / b` covers the division. Parentheses narrow
    /// it where that is not what was meant.
    Try(Box<Expr>),
    /// An anonymous function value: `fn(params) { .. }`.
    Function {
        params: Params,
        body: Vec<Stmt>,
    },
}

/// Prefix operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Negate, // -
    Not,    // !
}

/// Infix arithmetic and comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
}

/// Short-circuiting logical operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}
