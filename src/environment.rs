//! Lexical environments.
//!
//! A [`Scope`] maps names to values and points at an optional parent scope.
//! Scopes are shared through `Rc<RefCell<..>>` so a closure can capture the
//! exact scope it was defined in and keep it alive for as long as it lives.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::value::Value;

/// A shared, mutable handle to a scope.
pub type Env = Rc<RefCell<Scope>>;

#[derive(Default)]
pub struct Scope {
    values: HashMap<String, Value>,
    parent: Option<Env>,
}

/// Create a fresh, empty global scope.
pub fn new_global() -> Env {
    Rc::new(RefCell::new(Scope::default()))
}

/// Create a child scope that falls back to `parent` when a name is not found
/// locally.
pub fn new_child(parent: &Env) -> Env {
    Rc::new(RefCell::new(Scope {
        values: HashMap::new(),
        parent: Some(Rc::clone(parent)),
    }))
}

/// Define (or overwrite) a name in this scope only.
pub fn define(env: &Env, name: &str, value: Value) {
    env.borrow_mut().values.insert(name.to_string(), value);
}

/// Every binding defined directly in this scope, as owned pairs.
pub fn bindings(env: &Env) -> Vec<(String, Value)> {
    env.borrow()
        .values
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

/// Look up a name, walking up the scope chain. Returns `None` if undefined.
pub fn get(env: &Env, name: &str) -> Option<Value> {
    let parent = {
        let scope = env.borrow();
        if let Some(value) = scope.values.get(name) {
            return Some(value.clone());
        }
        scope.parent.clone()
    };
    parent.and_then(|parent| get(&parent, name))
}

/// Assign to the nearest existing binding of `name`. Returns `false` if the
/// name was never defined anywhere in the chain.
pub fn assign(env: &Env, name: &str, value: Value) -> bool {
    let parent = {
        let mut scope = env.borrow_mut();
        if scope.values.contains_key(name) {
            scope.values.insert(name.to_string(), value);
            return true;
        }
        scope.parent.clone()
    };
    match parent {
        Some(parent) => assign(&parent, name, value),
        None => false,
    }
}
