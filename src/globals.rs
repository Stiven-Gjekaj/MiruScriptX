//! The table of global variables, shared by the compiler and the virtual
//! machine.
//!
//! Globals used to live in a `HashMap<String, Value>` that the VM consulted by
//! name on every read and write, hashing a string each time. Since the compiler
//! already knows every name a program mentions, it can hand each one a slot
//! number once and emit that instead, turning a hash lookup into an index.
//!
//! The table is shared rather than per-program because a session compiles one
//! input at a time: `let x = 1` and a later `x` are compiled separately, and the
//! second has to resolve to the slot the first was given.
//!
//! A slot exists as soon as any program *mentions* a name, which is not the same
//! as the name having a value. Slots therefore hold `Option<Value>`, and reading
//! an empty one is the "undefined variable" error.

use std::collections::HashMap;

use crate::value::Value;

/// Global variables, addressed by slot.
#[derive(Default)]
pub struct Globals {
    values: Vec<Option<Value>>,
    /// Slot to name, kept so an error can name the variable it is about.
    names: Vec<String>,
    by_name: HashMap<String, u16>,
}

impl Globals {
    pub fn new() -> Globals {
        Globals::default()
    }

    /// The slot for `name`, assigning a fresh one the first time it is seen.
    /// Called by the compiler, so a slot may exist before anything is stored in
    /// it.
    pub fn slot_for(&mut self, name: &str) -> Option<u16> {
        if let Some(slot) = self.by_name.get(name) {
            return Some(*slot);
        }
        let slot = u16::try_from(self.values.len()).ok()?;
        self.values.push(None);
        self.names.push(name.to_string());
        self.by_name.insert(name.to_string(), slot);
        Some(slot)
    }

    /// Store a value in a slot, defining the variable.
    pub fn define(&mut self, slot: u16, value: Value) {
        self.values[slot as usize] = Some(value);
    }

    /// Read a slot, or `None` when the variable has not been defined.
    pub fn get(&self, slot: u16) -> Option<&Value> {
        self.values[slot as usize].as_ref()
    }

    /// Assign to a slot that already holds a value, reporting whether it did.
    pub fn assign(&mut self, slot: u16, value: Value) -> bool {
        let cell = &mut self.values[slot as usize];
        if cell.is_none() {
            return false;
        }
        *cell = Some(value);
        true
    }

    /// The name a slot was created for, for error messages.
    pub fn name(&self, slot: u16) -> &str {
        &self.names[slot as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_stable_and_reused_per_name() {
        let mut globals = Globals::new();
        let a = globals.slot_for("a").expect("a slot");
        let b = globals.slot_for("b").expect("a slot");
        assert_ne!(a, b);
        // Asking again gives the same slot, which is what lets a later program
        // resolve a name an earlier one defined.
        assert_eq!(globals.slot_for("a"), Some(a));
        assert_eq!(globals.name(a), "a");
    }

    #[test]
    fn a_slot_starts_undefined() {
        let mut globals = Globals::new();
        let slot = globals.slot_for("x").expect("a slot");
        assert!(globals.get(slot).is_none());
        // Assigning to an undefined slot fails rather than defining it.
        assert!(!globals.assign(slot, Value::Int(1)));
        globals.define(slot, Value::Int(1));
        assert!(globals.get(slot).is_some());
        assert!(globals.assign(slot, Value::Int(2)));
    }
}
