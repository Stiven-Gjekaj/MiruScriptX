//! The tree-walking interpreter: it evaluates the AST directly.
//!
//! Expressions produce [`Value`]s. Statements are executed for their effects
//! and yield a [`Flow`] so that `return` can unwind out of nested blocks and
//! function bodies. Every runtime error carries the line of the statement being
//! executed.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::io::Write;
use std::rc::Rc;

use crate::ast::{BinaryOp, Expr, LogicalOp, Stmt, StmtKind, UnaryOp};
use crate::environment::{self, Env};
use crate::value::{Function, Output, Value};
use crate::MiruError;

/// How execution should continue after a statement.
enum Flow {
    Normal,
    Return(Value),
}

/// A pair of numbers promoted to a common representation for arithmetic.
enum Num {
    Ints(i64, i64),
    Floats(f64, f64),
}

pub struct Interpreter {
    globals: Env,
    out: Box<dyn Write>,
    line: usize,
}

impl Default for Interpreter {
    fn default() -> Self {
        Interpreter::new()
    }
}

impl Output for Interpreter {
    fn write(&mut self, text: &str) {
        let _ = self.out.write_all(text.as_bytes());
    }
}

impl Interpreter {
    /// Create an interpreter that writes to standard output.
    pub fn new() -> Interpreter {
        Interpreter::with_output(Box::new(std::io::stdout()))
    }

    /// Create an interpreter that writes to a custom sink (used by tests and by
    /// the library's capture helpers).
    pub fn with_output(out: Box<dyn Write>) -> Interpreter {
        let globals = environment::new_global();
        Interpreter {
            globals,
            out,
            line: 0,
        }
    }

    /// Flush any buffered output.
    pub fn flush(&mut self) {
        let _ = self.out.flush();
    }

    /// Run a whole program. Returns the value of the final expression statement
    /// (or `nil`), which the REPL echoes back.
    pub fn run_program(&mut self, program: &[Stmt]) -> Result<Value, MiruError> {
        let env = Rc::clone(&self.globals);
        let mut last = Value::Nil;
        for stmt in program {
            self.line = stmt.line;
            if let StmtKind::Expr(expr) = &stmt.kind {
                last = self.eval(expr, &env)?;
            } else {
                match self.execute(stmt, &env)? {
                    Flow::Normal => last = Value::Nil,
                    Flow::Return(_) => break,
                }
            }
        }
        Ok(last)
    }

    // --- Statements -------------------------------------------------------

    fn execute(&mut self, stmt: &Stmt, env: &Env) -> Result<Flow, MiruError> {
        self.line = stmt.line;
        match &stmt.kind {
            StmtKind::Let { name, value } => {
                let value = self.eval(value, env)?;
                environment::define(env, name, value);
                Ok(Flow::Normal)
            }
            StmtKind::Assign { target, value } => {
                let value = self.eval(value, env)?;
                self.assign_target(target, value, env)?;
                Ok(Flow::Normal)
            }
            StmtKind::Expr(expr) => {
                self.eval(expr, env)?;
                Ok(Flow::Normal)
            }
            StmtKind::Return(expr) => {
                let value = match expr {
                    Some(expr) => self.eval(expr, env)?,
                    None => Value::Nil,
                };
                Ok(Flow::Return(value))
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if self.eval(condition, env)?.is_truthy() {
                    self.execute_block(then_branch, env)
                } else if let Some(else_branch) = else_branch {
                    self.execute_block(else_branch, env)
                } else {
                    Ok(Flow::Normal)
                }
            }
            StmtKind::While { condition, body } => {
                while self.eval(condition, env)?.is_truthy() {
                    if let Flow::Return(value) = self.execute_block(body, env)? {
                        return Ok(Flow::Return(value));
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => self.execute_for(name, iterable, body, env),
            StmtKind::Function { name, params, body } => {
                let function = Value::Function(Rc::new(Function {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: body.clone(),
                    closure: Rc::clone(env),
                }));
                environment::define(env, name, function);
                Ok(Flow::Normal)
            }
        }
    }

    fn execute_for(
        &mut self,
        name: &str,
        iterable: &Expr,
        body: &[Stmt],
        env: &Env,
    ) -> Result<Flow, MiruError> {
        let value = self.eval(iterable, env)?;
        let Value::Array(items) = value else {
            return Err(self.error(format!("cannot iterate over a {}", value.type_name())));
        };
        // Iterate over a snapshot so the loop is well-defined even if the body
        // mutates the original array.
        let snapshot: Vec<Value> = items.borrow().clone();
        for item in snapshot {
            let loop_env = environment::new_child(env);
            environment::define(&loop_env, name, item);
            if let Flow::Return(value) = self.execute_statements(body, &loop_env)? {
                return Ok(Flow::Return(value));
            }
        }
        Ok(Flow::Normal)
    }

    fn execute_block(&mut self, statements: &[Stmt], env: &Env) -> Result<Flow, MiruError> {
        let block_env = environment::new_child(env);
        self.execute_statements(statements, &block_env)
    }

    fn execute_statements(&mut self, statements: &[Stmt], env: &Env) -> Result<Flow, MiruError> {
        for stmt in statements {
            if let Flow::Return(value) = self.execute(stmt, env)? {
                return Ok(Flow::Return(value));
            }
        }
        Ok(Flow::Normal)
    }

    fn assign_target(&mut self, target: &Expr, value: Value, env: &Env) -> Result<(), MiruError> {
        match target {
            Expr::Identifier(name) => {
                if environment::assign(env, name, value) {
                    Ok(())
                } else {
                    Err(self.error(format!("cannot assign to undefined variable '{name}'")))
                }
            }
            Expr::Index { target, index } => {
                let target_value = self.eval(target, env)?;
                let index_value = self.eval(index, env)?;
                let Value::Array(items) = target_value else {
                    return Err(
                        self.error(format!("cannot index-assign to a {}", target_value.type_name()))
                    );
                };
                let len = items.borrow().len();
                let idx = self.as_index(&index_value, len)?;
                items.borrow_mut()[idx] = value;
                Ok(())
            }
            _ => Err(self.error("invalid assignment target")),
        }
    }

    // --- Expressions ------------------------------------------------------

    fn eval(&mut self, expr: &Expr, env: &Env) -> Result<Value, MiruError> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Str(s) => Ok(Value::Str(Rc::new(s.clone()))),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Nil => Ok(Value::Nil),
            Expr::Identifier(name) => environment::get(env, name)
                .ok_or_else(|| self.error(format!("undefined variable '{name}'"))),
            Expr::Array(elements) => {
                let mut items = Vec::with_capacity(elements.len());
                for element in elements {
                    items.push(self.eval(element, env)?);
                }
                Ok(Value::Array(Rc::new(RefCell::new(items))))
            }
            Expr::Index { target, index } => self.eval_index(target, index, env),
            Expr::Unary { op, operand } => {
                let value = self.eval(operand, env)?;
                self.eval_unary(*op, value)
            }
            Expr::Binary { op, left, right } => {
                let left = self.eval(left, env)?;
                let right = self.eval(right, env)?;
                self.eval_binary(*op, left, right)
            }
            Expr::Logical { op, left, right } => self.eval_logical(*op, left, right, env),
            Expr::Call { callee, arguments } => {
                let callee = self.eval(callee, env)?;
                let mut args = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    args.push(self.eval(argument, env)?);
                }
                self.call(callee, args)
            }
            Expr::Function { params, body } => Ok(Value::Function(Rc::new(Function {
                name: None,
                params: params.clone(),
                body: body.clone(),
                closure: Rc::clone(env),
            }))),
        }
    }

    fn call(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, MiruError> {
        match callee {
            Value::Function(func) => {
                if args.len() != func.params.len() {
                    return Err(self.error(format!(
                        "function {} expects {} argument(s) but received {}",
                        func.name.as_deref().unwrap_or("<anonymous>"),
                        func.params.len(),
                        args.len()
                    )));
                }
                let call_env = environment::new_child(&func.closure);
                for (param, arg) in func.params.iter().zip(args) {
                    environment::define(&call_env, param, arg);
                }
                match self.execute_statements(&func.body, &call_env)? {
                    Flow::Return(value) => Ok(value),
                    Flow::Normal => Ok(Value::Nil),
                }
            }
            Value::Builtin(builtin) => match (builtin.func)(self, args) {
                Ok(value) => Ok(value),
                Err(message) => Err(self.error(message)),
            },
            other => Err(self.error(format!("a {} is not callable", other.type_name()))),
        }
    }

    fn eval_index(&mut self, target: &Expr, index: &Expr, env: &Env) -> Result<Value, MiruError> {
        let target_value = self.eval(target, env)?;
        let index_value = self.eval(index, env)?;
        let Value::Array(items) = target_value else {
            return Err(self.error(format!("cannot index a {}", target_value.type_name())));
        };
        let len = items.borrow().len();
        let idx = self.as_index(&index_value, len)?;
        let element = items.borrow()[idx].clone();
        Ok(element)
    }

    fn as_index(&self, index: &Value, len: usize) -> Result<usize, MiruError> {
        let Value::Int(n) = index else {
            return Err(self.error(format!(
                "array index must be an int, not a {}",
                index.type_name()
            )));
        };
        if *n < 0 {
            return Err(self.error(format!("index {n} is out of range (negative)")));
        }
        let idx = *n as usize;
        if idx >= len {
            return Err(self.error(format!(
                "index {idx} is out of range for an array of length {len}"
            )));
        }
        Ok(idx)
    }

    fn eval_unary(&self, op: UnaryOp, value: Value) -> Result<Value, MiruError> {
        match op {
            UnaryOp::Negate => match value {
                Value::Int(n) => n
                    .checked_neg()
                    .map(Value::Int)
                    .ok_or_else(|| self.error("integer overflow while negating")),
                Value::Float(f) => Ok(Value::Float(-f)),
                other => Err(self.error(format!("cannot negate a {}", other.type_name()))),
            },
            UnaryOp::Not => Ok(Value::Bool(!value.is_truthy())),
        }
    }

    fn eval_binary(&self, op: BinaryOp, left: Value, right: Value) -> Result<Value, MiruError> {
        match op {
            BinaryOp::Add => self.eval_add(left, right),
            BinaryOp::Subtract => match self.numeric_pair(&left, &right, "subtract")? {
                Num::Ints(a, b) => a
                    .checked_sub(b)
                    .map(Value::Int)
                    .ok_or_else(|| self.error("integer overflow in subtraction")),
                Num::Floats(a, b) => Ok(Value::Float(a - b)),
            },
            BinaryOp::Multiply => match self.numeric_pair(&left, &right, "multiply")? {
                Num::Ints(a, b) => a
                    .checked_mul(b)
                    .map(Value::Int)
                    .ok_or_else(|| self.error("integer overflow in multiplication")),
                Num::Floats(a, b) => Ok(Value::Float(a * b)),
            },
            BinaryOp::Divide => match self.numeric_pair(&left, &right, "divide")? {
                Num::Ints(_, 0) => Err(self.error("division by zero")),
                Num::Ints(a, b) => a
                    .checked_div(b)
                    .map(Value::Int)
                    .ok_or_else(|| self.error("integer overflow in division")),
                Num::Floats(a, b) => {
                    if b == 0.0 {
                        Err(self.error("division by zero"))
                    } else {
                        Ok(Value::Float(a / b))
                    }
                }
            },
            BinaryOp::Modulo => match self.numeric_pair(&left, &right, "take the modulo of")? {
                Num::Ints(_, 0) => Err(self.error("modulo by zero")),
                Num::Ints(a, b) => a
                    .checked_rem(b)
                    .map(Value::Int)
                    .ok_or_else(|| self.error("integer overflow in modulo")),
                Num::Floats(a, b) => {
                    if b == 0.0 {
                        Err(self.error("modulo by zero"))
                    } else {
                        Ok(Value::Float(a % b))
                    }
                }
            },
            BinaryOp::Equal => Ok(Value::Bool(left.equals(&right))),
            BinaryOp::NotEqual => Ok(Value::Bool(!left.equals(&right))),
            BinaryOp::Less => Ok(Value::Bool(self.ordering(left, right)? == Ordering::Less)),
            BinaryOp::Greater => Ok(Value::Bool(self.ordering(left, right)? == Ordering::Greater)),
            BinaryOp::LessEqual => {
                Ok(Value::Bool(self.ordering(left, right)? != Ordering::Greater))
            }
            BinaryOp::GreaterEqual => {
                Ok(Value::Bool(self.ordering(left, right)? != Ordering::Less))
            }
        }
    }

    fn eval_add(&self, left: Value, right: Value) -> Result<Value, MiruError> {
        if let (Value::Str(a), Value::Str(b)) = (&left, &right) {
            let mut joined = String::with_capacity(a.len() + b.len());
            joined.push_str(a);
            joined.push_str(b);
            return Ok(Value::Str(Rc::new(joined)));
        }
        match self.numeric_pair(&left, &right, "add")? {
            Num::Ints(a, b) => a
                .checked_add(b)
                .map(Value::Int)
                .ok_or_else(|| self.error("integer overflow in addition")),
            Num::Floats(a, b) => Ok(Value::Float(a + b)),
        }
    }

    fn eval_logical(
        &mut self,
        op: LogicalOp,
        left: &Expr,
        right: &Expr,
        env: &Env,
    ) -> Result<Value, MiruError> {
        let left = self.eval(left, env)?;
        let result = match op {
            LogicalOp::And => left.is_truthy() && self.eval(right, env)?.is_truthy(),
            LogicalOp::Or => left.is_truthy() || self.eval(right, env)?.is_truthy(),
        };
        Ok(Value::Bool(result))
    }

    fn numeric_pair(&self, left: &Value, right: &Value, verb: &str) -> Result<Num, MiruError> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Num::Ints(*a, *b)),
            (Value::Int(a), Value::Float(b)) => Ok(Num::Floats(*a as f64, *b)),
            (Value::Float(a), Value::Int(b)) => Ok(Num::Floats(*a, *b as f64)),
            (Value::Float(a), Value::Float(b)) => Ok(Num::Floats(*a, *b)),
            _ => Err(self.error(format!(
                "cannot {} a {} and a {}",
                verb,
                left.type_name(),
                right.type_name()
            ))),
        }
    }

    fn ordering(&self, left: Value, right: Value) -> Result<Ordering, MiruError> {
        match (&left, &right) {
            (Value::Int(a), Value::Int(b)) => Ok(a.cmp(b)),
            (Value::Str(a), Value::Str(b)) => Ok(a.cmp(b)),
            _ => match self.numeric_pair(&left, &right, "compare")? {
                Num::Ints(a, b) => Ok(a.cmp(&b)),
                Num::Floats(a, b) => a
                    .partial_cmp(&b)
                    .ok_or_else(|| self.error("cannot compare with NaN")),
            },
        }
    }

    fn error(&self, message: impl Into<String>) -> MiruError {
        MiruError::new(self.line, message)
    }
}
