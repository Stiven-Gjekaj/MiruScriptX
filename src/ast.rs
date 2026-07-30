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
    /// `let name = value`
    Let { name: String, value: Expr },
    /// `target = value`, where `target` is an identifier or an index expression.
    Assign { target: Expr, value: Expr },
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
    /// `for name in iterable { .. }`
    For {
        name: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    /// A named function declaration: `fn name(params) { .. }`.
    Function {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
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
            ExprKind::Try(inner) => inner.height,
        };
        below + 1
    }
}

/// The different kinds of expressions in the language.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
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
    /// `try expr`. Evaluates the expression and yields its value, or, if
    /// evaluating it fails at any depth, the error itself as a value.
    ///
    /// Takes the whole expression that follows rather than binding tightly like
    /// a unary operator, so `try a / b` covers the division. Parentheses narrow
    /// it where that is not what was meant.
    Try(Box<Expr>),
    /// An anonymous function value: `fn(params) { .. }`.
    Function {
        params: Vec<String>,
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
