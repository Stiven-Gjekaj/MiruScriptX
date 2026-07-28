//! The arithmetic, comparison, and indexing rules at the heart of the language.
//!
//! Keeping them here rather than inline in the virtual machine means numeric
//! promotion, overflow checks, and index bounds are each defined exactly once.
//! Every function returns the bare error message on failure; the VM attaches
//! the source position from the chunk's position table.

use std::cmp::Ordering;
use std::rc::Rc;

use crate::ast::{BinaryOp, UnaryOp};
use crate::value::Value;

/// A pair of numbers promoted to a common representation for arithmetic.
enum Num {
    Ints(i64, i64),
    Floats(f64, f64),
}

/// The message for using a caught failure as if it were an ordinary value.
///
/// Reported where the misuse is written rather than where the failure happened,
/// because the misuse is the mistake: the program already had the failure in
/// hand and did something else with it. The original position stays on the
/// value, reachable through its fields, so nothing is lost by pointing here.
pub fn unhandled(value: &Value) -> Option<String> {
    match value {
        Value::Error(error) => Some(format!("unhandled failure: {}", error.message)),
        _ => None,
    }
}

/// Apply a unary operator to a value.
pub fn unary(op: UnaryOp, value: Value) -> Result<Value, String> {
    // Checked here rather than in each rule below, which matters most for
    // `Not`: it answers for every value through `is_truthy` and so would
    // otherwise quietly report a failure as `false`.
    if let Some(message) = unhandled(&value) {
        return Err(message);
    }
    match op {
        UnaryOp::Negate => match value {
            Value::Int(n) => n
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| "integer overflow while negating".to_string()),
            Value::Float(f) => Ok(Value::Float(-f)),
            other => Err(format!("cannot negate a {}", other.type_name())),
        },
        UnaryOp::Not => Ok(Value::Bool(!value.is_truthy())),
    }
}

/// Apply a binary operator to two values.
pub fn binary(op: BinaryOp, left: Value, right: Value) -> Result<Value, String> {
    // Once, here, rather than in each rule. `Equal` and `NotEqual` are the ones
    // that need it: they answer for every pair through `Value::equals`, so
    // without this a failure would compare `false` against everything and look
    // like an ordinary value that simply did not match.
    //
    // The VM's integer fast path has already declined by the time this runs, so
    // no arithmetic between two integers reaches it.
    if let Some(message) = unhandled(&left).or_else(|| unhandled(&right)) {
        return Err(message);
    }
    match op {
        BinaryOp::Add => add(left, right),
        BinaryOp::Subtract => match numeric_pair(&left, &right, "subtract")? {
            Num::Ints(a, b) => a
                .checked_sub(b)
                .map(Value::Int)
                .ok_or_else(|| "integer overflow in subtraction".to_string()),
            Num::Floats(a, b) => Ok(Value::Float(a - b)),
        },
        BinaryOp::Multiply => match numeric_pair(&left, &right, "multiply")? {
            Num::Ints(a, b) => a
                .checked_mul(b)
                .map(Value::Int)
                .ok_or_else(|| "integer overflow in multiplication".to_string()),
            Num::Floats(a, b) => Ok(Value::Float(a * b)),
        },
        BinaryOp::Divide => match numeric_pair(&left, &right, "divide")? {
            Num::Ints(_, 0) => Err("division by zero".to_string()),
            Num::Ints(a, b) => a
                .checked_div(b)
                .map(Value::Int)
                .ok_or_else(|| "integer overflow in division".to_string()),
            Num::Floats(a, b) => {
                if b == 0.0 {
                    Err("division by zero".to_string())
                } else {
                    Ok(Value::Float(a / b))
                }
            }
        },
        BinaryOp::Modulo => match numeric_pair(&left, &right, "take the modulo of")? {
            Num::Ints(_, 0) => Err("modulo by zero".to_string()),
            Num::Ints(a, b) => a
                .checked_rem(b)
                .map(Value::Int)
                .ok_or_else(|| "integer overflow in modulo".to_string()),
            Num::Floats(a, b) => {
                if b == 0.0 {
                    Err("modulo by zero".to_string())
                } else {
                    Ok(Value::Float(a % b))
                }
            }
        },
        BinaryOp::Equal => Ok(Value::Bool(left.equals(&right))),
        BinaryOp::NotEqual => Ok(Value::Bool(!left.equals(&right))),
        BinaryOp::Less => Ok(Value::Bool(ordering(left, right)? == Ordering::Less)),
        BinaryOp::Greater => Ok(Value::Bool(ordering(left, right)? == Ordering::Greater)),
        BinaryOp::LessEqual => Ok(Value::Bool(ordering(left, right)? != Ordering::Greater)),
        BinaryOp::GreaterEqual => Ok(Value::Bool(ordering(left, right)? != Ordering::Less)),
    }
}

/// Addition doubles as string concatenation when both operands are strings.
fn add(left: Value, right: Value) -> Result<Value, String> {
    if let (Value::Str(a), Value::Str(b)) = (&left, &right) {
        let mut joined = String::with_capacity(a.len() + b.len());
        joined.push_str(a);
        joined.push_str(b);
        return Ok(Value::Str(Rc::new(joined)));
    }
    match numeric_pair(&left, &right, "add")? {
        Num::Ints(a, b) => a
            .checked_add(b)
            .map(Value::Int)
            .ok_or_else(|| "integer overflow in addition".to_string()),
        Num::Floats(a, b) => Ok(Value::Float(a + b)),
    }
}

/// Promote two operands to a common numeric type, or fail with a message naming
/// the attempted operation.
fn numeric_pair(left: &Value, right: &Value, verb: &str) -> Result<Num, String> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => Ok(Num::Ints(*a, *b)),
        (Value::Int(a), Value::Float(b)) => Ok(Num::Floats(*a as f64, *b)),
        (Value::Float(a), Value::Int(b)) => Ok(Num::Floats(*a, *b as f64)),
        (Value::Float(a), Value::Float(b)) => Ok(Num::Floats(*a, *b)),
        _ => Err(format!(
            "cannot {} a {} and a {}",
            verb,
            left.type_name(),
            right.type_name()
        )),
    }
}

/// Validate an array index: it must be a non-negative integer within bounds.
/// Returns the usize index, or a message naming the problem.
pub fn array_index(index: &Value, len: usize) -> Result<usize, String> {
    if let Some(message) = unhandled(index) {
        return Err(message);
    }
    let Value::Int(n) = index else {
        return Err(format!(
            "array index must be an int, not a {}",
            index.type_name()
        ));
    };
    if *n < 0 {
        return Err(format!("index {n} is out of range (negative)"));
    }
    let index = *n as usize;
    if index >= len {
        return Err(format!(
            "index {index} is out of range for an array of length {len}"
        ));
    }
    Ok(index)
}

/// Validate a map key: it must be a string.
pub fn map_key(index: &Value) -> Result<String, String> {
    match index {
        Value::Str(s) => Ok(s.to_string()),
        Value::Error(error) => Err(format!("unhandled failure: {}", error.message)),
        other => Err(format!(
            "map key must be a string, not a {}",
            other.type_name()
        )),
    }
}

/// Order two values: integers and strings compare directly, mixed numbers are
/// promoted, and NaN is rejected.
fn ordering(left: Value, right: Value) -> Result<Ordering, String> {
    match (&left, &right) {
        (Value::Int(a), Value::Int(b)) => Ok(a.cmp(b)),
        (Value::Str(a), Value::Str(b)) => Ok(a.cmp(b)),
        _ => match numeric_pair(&left, &right, "compare")? {
            Num::Ints(a, b) => Ok(a.cmp(&b)),
            Num::Floats(a, b) => a
                .partial_cmp(&b)
                .ok_or_else(|| "cannot compare with NaN".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure() -> Value {
        Value::Error(Rc::new(crate::MiruError::with_column(
            1,
            9,
            "division by zero",
        )))
    }

    #[test]
    fn every_operator_refuses_a_caught_failure_as_an_operand() {
        // The two that need saying out loud, because neither reports a type
        // error for anything: `!` answers for every value through is_truthy,
        // and `==` answers for every pair through Value::equals. Left to
        // themselves they would call a failure `false` and "not equal to
        // anything", which reads exactly like an ordinary value.
        assert_eq!(
            unary(UnaryOp::Not, failure()).err().unwrap(),
            "unhandled failure: division by zero"
        );
        assert_eq!(
            binary(BinaryOp::Equal, failure(), Value::Int(1))
                .err()
                .unwrap(),
            "unhandled failure: division by zero"
        );

        // And the ordinary ones, on either side.
        for op in [
            BinaryOp::Add,
            BinaryOp::Subtract,
            BinaryOp::Multiply,
            BinaryOp::Divide,
            BinaryOp::Modulo,
            BinaryOp::NotEqual,
            BinaryOp::Less,
            BinaryOp::Greater,
            BinaryOp::LessEqual,
            BinaryOp::GreaterEqual,
        ] {
            assert_eq!(
                binary(op, failure(), Value::Int(1)).err().unwrap(),
                "unhandled failure: division by zero",
                "{op:?} accepted a failure on the left"
            );
            assert_eq!(
                binary(op, Value::Int(1), failure()).err().unwrap(),
                "unhandled failure: division by zero",
                "{op:?} accepted a failure on the right"
            );
        }
        assert_eq!(
            unary(UnaryOp::Negate, failure()).err().unwrap(),
            "unhandled failure: division by zero"
        );
    }

    #[test]
    fn a_caught_failure_is_not_an_index_or_a_key() {
        assert_eq!(
            array_index(&failure(), 3).err().unwrap(),
            "unhandled failure: division by zero"
        );
        assert_eq!(
            map_key(&failure()).err().unwrap(),
            "unhandled failure: division by zero"
        );
    }

    #[test]
    fn arithmetic_promotes_and_concatenates() {
        assert!(binary(BinaryOp::Add, Value::Int(1), Value::Int(2))
            .unwrap()
            .equals(&Value::Int(3)));
        assert!(binary(BinaryOp::Add, Value::Int(1), Value::Float(2.0))
            .unwrap()
            .equals(&Value::Float(3.0)));
        let joined = binary(
            BinaryOp::Add,
            Value::Str(Rc::new("a".to_string())),
            Value::Str(Rc::new("b".to_string())),
        )
        .unwrap();
        assert!(joined.equals(&Value::Str(Rc::new("ab".to_string()))));
    }

    #[test]
    fn division_and_modulo_by_zero_are_errors() {
        assert_eq!(
            binary(BinaryOp::Divide, Value::Int(1), Value::Int(0))
                .err()
                .unwrap(),
            "division by zero"
        );
        assert_eq!(
            binary(BinaryOp::Modulo, Value::Int(1), Value::Int(0))
                .err()
                .unwrap(),
            "modulo by zero"
        );
    }

    #[test]
    fn integer_overflow_is_reported() {
        assert_eq!(
            binary(BinaryOp::Add, Value::Int(i64::MAX), Value::Int(1))
                .err()
                .unwrap(),
            "integer overflow in addition"
        );
        assert_eq!(
            unary(UnaryOp::Negate, Value::Int(i64::MIN)).err().unwrap(),
            "integer overflow while negating"
        );
    }

    #[test]
    fn comparisons_and_type_errors() {
        assert!(binary(BinaryOp::Less, Value::Int(1), Value::Int(2))
            .unwrap()
            .equals(&Value::Bool(true)));
        assert_eq!(
            binary(BinaryOp::Add, Value::Int(1), Value::Bool(true))
                .err()
                .unwrap(),
            "cannot add a int and a bool"
        );
    }

    #[test]
    fn not_and_negate() {
        assert!(unary(UnaryOp::Not, Value::Nil)
            .unwrap()
            .equals(&Value::Bool(true)));
        assert!(unary(UnaryOp::Negate, Value::Int(5))
            .unwrap()
            .equals(&Value::Int(-5)));
    }
}
