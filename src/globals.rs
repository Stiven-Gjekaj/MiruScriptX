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
//!
//! # Modules share slots but not names
//!
//! Each module gets its own name-to-slot map, while slots stay globally unique
//! indices into one table. Two files can therefore both define `total` without
//! colliding, and `GetGlobal` is still a single indexed read, because what is
//! namespaced is resolution at compile time rather than access at run time.
//!
//! Builtins are kept apart from all of that, in a map every module can see. An
//! imported module must be able to call `print` without being able to see the
//! importing file's variables, and those are two different questions.

use std::collections::{BTreeMap, HashMap};

use crate::value::Value;

/// Which module a name is being resolved in.
pub type ModuleId = usize;

/// The module a single-file program compiles into, and the one a REPL session
/// keeps using across inputs.
pub const ROOT_MODULE: ModuleId = 0;

/// Global variables, addressed by slot.
pub struct Globals {
    values: Vec<Option<Value>>,
    /// Slot to name, kept so an error can name the variable it is about.
    names: Vec<String>,
    /// Builtin names, visible from every module.
    builtins: HashMap<String, u16>,
    /// One name-to-slot map per module, indexed by [`ModuleId`].
    modules: Vec<HashMap<String, u16>>,
}

impl Default for Globals {
    fn default() -> Self {
        Globals::new()
    }
}

impl Globals {
    pub fn new() -> Globals {
        Globals {
            values: Vec::new(),
            names: Vec::new(),
            builtins: HashMap::new(),
            // The root module always exists, so `ROOT_MODULE` is valid before
            // anything has been compiled.
            modules: vec![HashMap::new()],
        }
    }

    /// Start a new module and return the id to resolve its names against.
    pub fn new_module(&mut self) -> ModuleId {
        self.modules.push(HashMap::new());
        self.modules.len() - 1
    }

    /// The slot for `name` as seen from `module`, assigning a fresh one the
    /// first time that module mentions it. Called by the compiler, so a slot may
    /// exist before anything is stored in it.
    ///
    /// A builtin's name resolves the same from every module, and resolves ahead
    /// of the module's own names, which is what keeps `let print = 1` writing
    /// over the builtin rather than shadowing it. That was how a single flat
    /// table behaved, and modules were not a reason to change it.
    pub fn slot_for(&mut self, module: ModuleId, name: &str) -> Option<u16> {
        if let Some(slot) = self.builtins.get(name) {
            return Some(*slot);
        }
        if let Some(slot) = self.modules[module].get(name) {
            return Some(*slot);
        }
        let slot = u16::try_from(self.values.len()).ok()?;
        self.values.push(None);
        self.names.push(name.to_string());
        self.modules[module].insert(name.to_string(), slot);
        Some(slot)
    }

    /// Reserve the slot for a builtin, which every module can reach.
    pub fn slot_for_builtin(&mut self, name: &str) -> Option<u16> {
        if let Some(slot) = self.builtins.get(name) {
            return Some(*slot);
        }
        let slot = u16::try_from(self.values.len()).ok()?;
        self.values.push(None);
        self.names.push(name.to_string());
        self.builtins.insert(name.to_string(), slot);
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

    /// Whether a name has a slot in this table.
    ///
    /// Unlike [`Globals::slot_for`] this only asks, and so takes `&self`: it is
    /// a membership test rather than a step towards emitting code. The
    /// playground uses it to colour builtins, by populating a table with
    /// [`crate::builtins::register`] and asking it, rather than keeping a second
    /// list of builtin names that could fall out of step with the real one.
    pub fn contains(&self, name: &str) -> bool {
        self.builtins.contains_key(name) || self.modules[ROOT_MODULE].contains_key(name)
    }

    /// A module's top-level names and their values.
    ///
    /// Everything the module defined is visible to whoever imports it. A name
    /// that has a slot but no value was mentioned and never defined, so it is
    /// left out rather than exported as `nil`: a typo inside a module should
    /// not become a field on it.
    pub fn exports(&self, module: ModuleId) -> BTreeMap<String, Value> {
        self.modules[module]
            .iter()
            .filter_map(|(name, slot)| {
                self.values[*slot as usize]
                    .clone()
                    .map(|value| (name.clone(), value))
            })
            .collect()
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
        let a = globals.slot_for(ROOT_MODULE, "a").expect("a slot");
        let b = globals.slot_for(ROOT_MODULE, "b").expect("a slot");
        assert_ne!(a, b);
        // Asking again gives the same slot, which is what lets a later program
        // resolve a name an earlier one defined.
        assert_eq!(globals.slot_for(ROOT_MODULE, "a"), Some(a));
        assert_eq!(globals.name(a), "a");
    }

    #[test]
    fn two_modules_can_define_the_same_name() {
        let mut globals = Globals::new();
        let other = globals.new_module();
        let root_total = globals.slot_for(ROOT_MODULE, "total").expect("a slot");
        let other_total = globals.slot_for(other, "total").expect("a slot");
        // Same name, different slots, so neither file can see or clobber the
        // other's variable.
        assert_ne!(root_total, other_total);
        globals.define(root_total, Value::Int(1));
        globals.define(other_total, Value::Int(2));
        assert_eq!(
            globals.get(root_total).map(Value::repr),
            Some("1".to_string())
        );
        assert_eq!(
            globals.get(other_total).map(Value::repr),
            Some("2".to_string())
        );
        // Asking again gives each module its own slot back, which is what lets
        // a later input resolve a name an earlier one defined.
        assert_eq!(globals.slot_for(ROOT_MODULE, "total"), Some(root_total));
        assert_eq!(globals.slot_for(other, "total"), Some(other_total));
    }

    #[test]
    fn a_builtin_resolves_the_same_from_every_module() {
        let mut globals = Globals::new();
        crate::builtins::register(&mut globals);
        let other = globals.new_module();
        // An imported file must be able to call print without being able to see
        // the importing file's variables, so builtins live apart from both.
        assert_eq!(
            globals.slot_for(ROOT_MODULE, "print"),
            globals.slot_for(other, "print")
        );
        // And a program that defines its own `print` still writes over the
        // builtin rather than shadowing it, which is what the flat table did.
        let slot = globals.slot_for(ROOT_MODULE, "print").expect("a slot");
        globals.define(slot, Value::Int(5));
        assert_eq!(globals.get(slot).map(Value::repr), Some("5".to_string()));
    }

    #[test]
    fn contains_reports_membership_without_creating_a_slot() {
        let mut globals = Globals::new();
        assert!(!globals.contains("print"));
        crate::builtins::register(&mut globals);
        assert!(globals.contains("print"));
        // Asking does not invent a slot, which is what separates this from
        // slot_for.
        assert!(!globals.contains("definitely_not_a_builtin"));
    }

    #[test]
    fn a_slot_starts_undefined() {
        let mut globals = Globals::new();
        let slot = globals.slot_for(ROOT_MODULE, "x").expect("a slot");
        assert!(globals.get(slot).is_none());
        // Assigning to an undefined slot fails rather than defining it.
        assert!(!globals.assign(slot, Value::Int(1)));
        globals.define(slot, Value::Int(1));
        assert!(globals.get(slot).is_some());
        assert!(globals.assign(slot, Value::Int(2)));
    }
}
