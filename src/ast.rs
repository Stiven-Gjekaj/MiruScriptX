//! The abstract syntax tree (AST) that the parser produces and the interpreter
//! walks. Statements carry the source line they begin on so runtime errors can
//! point at the right place.

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
}

impl Expr {
    pub fn new(kind: ExprKind, line: usize, column: usize) -> Expr {
        Expr { kind, line, column }
    }
}

/// Two expressions are equal when their kinds match; positions are ignored so
/// tests can compare tree shape without tracking every line and column.
impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
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
