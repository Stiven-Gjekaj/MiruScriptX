//! The tree-walking interpreter: it evaluates the AST directly.
//!
//! Expressions produce [`Value`]s. Statements are executed for their effects
//! and yield a [`Flow`] so that `return` can unwind out of nested blocks and
//! function bodies. Every runtime error carries the line of the statement being
//! executed.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::Write;
use std::rc::Rc;

use crate::ast::{BinaryOp, Expr, ExprKind, LogicalOp, Stmt, StmtKind, UnaryOp};
use crate::environment::{self, Env};
use crate::value::{EmptyInput, Function, Input, Output, Value};
use crate::MiruError;

/// How execution should continue after a statement.
enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

/// A pair of numbers promoted to a common representation for arithmetic.
enum Num {
    Ints(i64, i64),
    Floats(f64, f64),
}

pub struct Interpreter {
    globals: Env,
    out: Box<dyn Write>,
    input: Box<dyn Input>,
    line: usize,
    column: usize,
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
        crate::builtins::register(&globals);
        Interpreter {
            globals,
            out,
            input: Box::new(EmptyInput),
            line: 0,
            column: 0,
        }
    }

    /// Flush any buffered output.
    pub fn flush(&mut self) {
        let _ = self.out.flush();
    }

    /// Replace the input source. The binary points this at standard input; the
    /// capture helpers point it at a scripted buffer.
    pub fn set_input(&mut self, input: Box<dyn Input>) {
        self.input = input;
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
                    Flow::Break | Flow::Continue => {
                        return Err(self.error("break or continue outside of a loop"));
                    }
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
            StmtKind::Break => Ok(Flow::Break),
            StmtKind::Continue => Ok(Flow::Continue),
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
                    match self.execute_block(body, env)? {
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal => {}
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
            match self.execute_statements(body, &loop_env)? {
                Flow::Return(value) => return Ok(Flow::Return(value)),
                Flow::Break => break,
                Flow::Continue | Flow::Normal => {}
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
            match self.execute(stmt, env)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn assign_target(&mut self, target: &Expr, value: Value, env: &Env) -> Result<(), MiruError> {
        self.line = target.line;
        self.column = target.column;
        match &target.kind {
            ExprKind::Identifier(name) => {
                if environment::assign(env, name, value) {
                    Ok(())
                } else {
                    Err(self.error(format!("cannot assign to undefined variable '{name}'")))
                }
            }
            ExprKind::Index { target, index } => {
                let target_value = self.eval(target, env)?;
                let index_value = self.eval(index, env)?;
                match target_value {
                    Value::Array(items) => {
                        let len = items.borrow().len();
                        let idx = self.as_index(&index_value, len)?;
                        items.borrow_mut()[idx] = value;
                        Ok(())
                    }
                    Value::Map(entries) => {
                        let key = self.as_map_key(&index_value)?;
                        entries.borrow_mut().insert(key, value);
                        Ok(())
                    }
                    other => {
                        self.line = target.line;
                        self.column = target.column;
                        Err(self.error(format!("cannot index-assign to a {}", other.type_name())))
                    }
                }
            }
            _ => Err(self.error("invalid assignment target")),
        }
    }

    // --- Expressions ------------------------------------------------------

    fn eval(&mut self, expr: &Expr, env: &Env) -> Result<Value, MiruError> {
        self.line = expr.line;
        self.column = expr.column;
        match &expr.kind {
            ExprKind::Int(n) => Ok(Value::Int(*n)),
            ExprKind::Float(f) => Ok(Value::Float(*f)),
            ExprKind::Str(s) => Ok(Value::Str(Rc::new(s.clone()))),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Nil => Ok(Value::Nil),
            ExprKind::Identifier(name) => environment::get(env, name)
                .ok_or_else(|| self.error(format!("undefined variable '{name}'"))),
            ExprKind::Array(elements) => {
                let mut items = Vec::with_capacity(elements.len());
                for element in elements {
                    items.push(self.eval(element, env)?);
                }
                Ok(Value::Array(Rc::new(RefCell::new(items))))
            }
            ExprKind::Map(entries) => {
                let mut map = BTreeMap::new();
                for (key_expr, value_expr) in entries {
                    let key = self.eval(key_expr, env)?;
                    let Value::Str(key) = key else {
                        return Err(self.error(format!(
                            "map key must be a string, not a {}",
                            key.type_name()
                        )));
                    };
                    let value = self.eval(value_expr, env)?;
                    map.insert(key.to_string(), value);
                }
                Ok(Value::Map(Rc::new(RefCell::new(map))))
            }
            ExprKind::Index { target, index } => self.eval_index(target, index, env),
            ExprKind::Unary { op, operand } => {
                let value = self.eval(operand, env)?;
                self.line = expr.line;
                self.column = expr.column;
                self.eval_unary(*op, value)
            }
            ExprKind::Binary { op, left, right } => {
                let left = self.eval(left, env)?;
                let right = self.eval(right, env)?;
                self.line = expr.line;
                self.column = expr.column;
                self.eval_binary(*op, left, right)
            }
            ExprKind::Logical { op, left, right } => self.eval_logical(*op, left, right, env),
            ExprKind::Call { callee, arguments } => {
                let callee = self.eval(callee, env)?;
                let mut args = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    args.push(self.eval(argument, env)?);
                }
                self.line = expr.line;
                self.column = expr.column;
                self.call(callee, args)
            }
            ExprKind::Function { params, body } => Ok(Value::Function(Rc::new(Function {
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
                    Flow::Break | Flow::Continue => {
                        Err(self.error("break or continue outside of a loop"))
                    }
                }
            }
            Value::Builtin(builtin) => {
                // Move the input reader out so the interpreter can also be
                // borrowed as the output sink during the call, then restore it.
                let mut input = std::mem::replace(&mut self.input, Box::new(EmptyInput));
                let result = (builtin.func)(self, &mut *input, args);
                self.input = input;
                match result {
                    Ok(value) => Ok(value),
                    Err(message) => Err(self.error(message)),
                }
            }
            other => Err(self.error(format!("a {} is not callable", other.type_name()))),
        }
    }

    fn eval_index(&mut self, target: &Expr, index: &Expr, env: &Env) -> Result<Value, MiruError> {
        let target_value = self.eval(target, env)?;
        let index_value = self.eval(index, env)?;
        match target_value {
            Value::Array(items) => {
                let len = items.borrow().len();
                let idx = self.as_index(&index_value, len)?;
                let element = items.borrow()[idx].clone();
                Ok(element)
            }
            Value::Map(entries) => {
                let key = self.as_map_key(&index_value)?;
                Ok(entries.borrow().get(&key).cloned().unwrap_or(Value::Nil))
            }
            other => {
                self.line = target.line;
                self.column = target.column;
                Err(self.error(format!("cannot index a {}", other.type_name())))
            }
        }
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

    fn as_map_key(&self, index: &Value) -> Result<String, MiruError> {
        match index {
            Value::Str(s) => Ok(s.to_string()),
            other => Err(self.error(format!(
                "map key must be a string, not a {}",
                other.type_name()
            ))),
        }
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
            BinaryOp::Greater => Ok(Value::Bool(
                self.ordering(left, right)? == Ordering::Greater,
            )),
            BinaryOp::LessEqual => Ok(Value::Bool(
                self.ordering(left, right)? != Ordering::Greater,
            )),
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
        MiruError::with_column(self.line, self.column, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    fn run(source: &str) -> Value {
        let program = crate::parse_program(source).expect("source should parse");
        let mut interpreter = Interpreter::with_output(Box::new(Vec::new()));
        interpreter
            .run_program(&program)
            .expect("program should run")
    }

    fn repr(source: &str) -> String {
        run(source).repr()
    }

    fn error(source: &str) -> MiruError {
        let program = crate::parse_program(source).expect("source should parse");
        let mut interpreter = Interpreter::with_output(Box::new(Vec::new()));
        match interpreter.run_program(&program) {
            Ok(value) => panic!(
                "expected an error but the program returned {}",
                value.repr()
            ),
            Err(err) => err,
        }
    }

    #[test]
    fn runtime_error_points_at_the_operator() {
        // The '/' sits at column 3, where the division by zero happens.
        let err = error("1 / 0");
        assert_eq!(err.line, 1);
        assert_eq!(err.column, 3);
    }

    #[test]
    fn undefined_variable_points_at_the_name() {
        let err = error("  nope");
        assert_eq!(err.column, 3);
    }

    #[test]
    fn out_of_range_index_points_at_the_index() {
        // The index expression 5 sits at column 7, where the lookup fails.
        let err = error("[1][  5]");
        assert_eq!(err.column, 7);
    }

    #[test]
    fn arithmetic_precedence() {
        assert_eq!(repr("1 + 2 * 3"), "7");
        assert_eq!(repr("(1 + 2) * 3"), "9");
    }

    #[test]
    fn integer_division_truncates_but_floats_do_not() {
        assert_eq!(repr("10 / 3"), "3");
        assert_eq!(repr("10.0 / 4"), "2.5");
    }

    #[test]
    fn numeric_promotion_produces_floats() {
        assert_eq!(repr("2 + 3.0"), "5.0");
        assert_eq!(repr("2.0"), "2.0");
    }

    #[test]
    fn string_concatenation() {
        assert_eq!(repr("\"Hello, \" + \"world\""), "\"Hello, world\"");
    }

    #[test]
    fn comparisons_and_logic() {
        assert_eq!(repr("1 < 2"), "true");
        assert_eq!(repr("1 == 1.0"), "true");
        assert_eq!(repr("true && false"), "false");
        assert_eq!(repr("false || true"), "true");
        assert_eq!(repr("!nil"), "true");
    }

    #[test]
    fn variables_and_reassignment() {
        assert_eq!(repr("let x = 5\nx = x + 1\nx"), "6");
    }

    #[test]
    fn arrays_index_and_assign() {
        assert_eq!(repr("let a = [1, 2, 3]\na[1]"), "2");
        assert_eq!(repr("let a = [1, 2, 3]\na[0] = 9\na"), "[9, 2, 3]");
    }

    #[test]
    fn for_loop_sums_an_array() {
        assert_eq!(
            repr("let total = 0\nfor x in [1, 2, 3, 4] {\n  total = total + x\n}\ntotal"),
            "10"
        );
    }

    #[test]
    fn while_loop_counts() {
        assert_eq!(
            repr("let i = 0\nlet sum = 0\nwhile i < 5 {\n  sum = sum + i\n  i = i + 1\n}\nsum"),
            "10"
        );
    }

    #[test]
    fn if_else_selects_a_branch() {
        let source =
            "let out = \"\"\nlet x = 7\nif x % 2 == 0 {\n  out = \"even\"\n} else {\n  out = \"odd\"\n}\nout";
        assert_eq!(repr(source), "\"odd\"");
    }

    #[test]
    fn recursive_fibonacci() {
        let source = "fn fib(n) {\n  if n < 2 {\n    return n\n  }\n  return fib(n - 1) + fib(n - 2)\n}\nfib(10)";
        assert_eq!(repr(source), "55");
    }

    #[test]
    fn closures_capture_their_environment() {
        let source =
            "fn make_adder(x) {\n  return fn(y) {\n    return x + y\n  }\n}\nlet add5 = make_adder(5)\nadd5(3)";
        assert_eq!(repr(source), "8");
    }

    #[test]
    fn a_function_without_return_yields_nil() {
        assert_eq!(repr("fn noop() {}\nnoop()"), "nil");
    }

    #[test]
    fn reports_undefined_variable_with_line() {
        let err = error("let a = 1\nb");
        assert_eq!(err.line, 2);
        assert!(err.message.contains("undefined variable 'b'"));
    }

    #[test]
    fn reports_division_by_zero() {
        assert!(error("1 / 0").message.contains("division by zero"));
    }

    #[test]
    fn reports_type_errors() {
        assert!(error("\"a\" - 1").message.contains("cannot subtract"));
    }

    #[test]
    fn reports_index_out_of_range() {
        assert!(error("let a = [1]\na[5]").message.contains("out of range"));
    }

    #[test]
    fn reports_wrong_argument_count() {
        let err = error("fn f(a) {\n  return a\n}\nf(1, 2)");
        assert!(err.message.contains("expects 1 argument"));
    }

    #[test]
    fn reports_calling_a_non_function() {
        assert!(error("let x = 5\nx()").message.contains("not callable"));
    }

    #[test]
    fn break_stops_a_loop() {
        let source =
            "let last = 0\nfor n in range(1, 10) {\n  if n == 5 { break }\n  last = n\n}\nlast";
        assert_eq!(repr(source), "4");
    }

    #[test]
    fn continue_skips_to_the_next_iteration() {
        let source =
            "let sum = 0\nfor n in range(1, 6) {\n  if n % 2 == 0 { continue }\n  sum = sum + n\n}\nsum";
        assert_eq!(repr(source), "9");
    }

    #[test]
    fn break_works_in_a_while_loop() {
        let source = "let i = 0\nwhile true {\n  i = i + 1\n  if i == 3 { break }\n}\ni";
        assert_eq!(repr(source), "3");
    }

    #[test]
    fn break_only_exits_the_inner_loop() {
        let source = "let count = 0\nfor a in range(0, 3) {\n  for b in range(0, 3) {\n    if b == 1 { break }\n    count = count + 1\n  }\n}\ncount";
        assert_eq!(repr(source), "3");
    }

    #[test]
    fn evaluates_map_literals_in_sorted_order() {
        assert_eq!(
            repr("{\"name\": \"Aiko\", \"age\": 3}"),
            "{\"age\": 3, \"name\": \"Aiko\"}"
        );
    }

    #[test]
    fn evaluates_an_empty_map() {
        assert_eq!(repr("{}"), "{}");
    }

    #[test]
    fn map_key_must_be_a_string() {
        assert!(error("{1: 2}").message.contains("map key must be a string"));
    }

    #[test]
    fn reads_and_writes_map_values() {
        let source = "let m = {\"a\": 1}\nm[\"b\"] = 2\nm[\"a\"] = 10\nm";
        assert_eq!(repr(source), "{\"a\": 10, \"b\": 2}");
    }

    #[test]
    fn missing_map_key_reads_as_nil() {
        assert_eq!(repr("let m = {\"a\": 1}\nm[\"z\"]"), "nil");
    }

    #[test]
    fn map_index_key_must_be_a_string() {
        assert!(error("let m = {\"a\": 1}\nm[3]")
            .message
            .contains("map key must be a string"));
    }
}
