//! `miru migrate`: what version 2.0 changes about a program, and the half of
//! that a tool can change for you.
//!
//! **Why this ships in the last 1.x rather than in 2.0.** Version 2.0 reserves
//! sixteen words that are ordinary identifiers today. A program that calls a
//! variable `match` is a syntax error under the 2.0 grammar, so the 2.0 binary
//! cannot read the file it is supposed to fix. Only a 1.x parser can, which
//! makes this the last thing the 1.x line is for.
//!
//! **What it will and will not do.** It renames, because a rename is decidable:
//! a name is a name, and the tool can see every place it is written. It refuses
//! to touch a call whose *meaning* changes, because deciding those needs to
//! know what a value will be at run time. Those come back as notes with a line
//! number, for a person to read.

use std::collections::{HashMap, HashSet};

use crate::ast::{Expr, ExprKind, Pattern, Stmt, StmtKind, UnaryOp};
use crate::lexer::Lexer;
use crate::token::{FStringPart, TokenKind};
use crate::MiruError;

/// The words version 2.0 makes keywords, and that a program may still use as
/// names today.
///
/// Sixteen words are keywords already, so this doubles the count in one
/// release. That is deliberate: a language that reserves one word per major
/// version spends a major on every feature that needs a word. The reason each
/// of these is here is recorded in the 2.0 notes rather than in code, but the
/// short version is that every one of them is wanted by a construct somebody
/// has actually asked for.
///
/// **`type` is not on this list although it was.** It is the only candidate
/// that collides with a builtin, and reserving it would delete `type(x)` from
/// the language.
pub const RESERVED_IN_2: [&str; 16] = [
    "async", "await", "case", "const", "default", "defer", "enum", "finally", "is", "loop",
    "match", "pub", "struct", "until", "use", "yield",
];

/// One name the tool renamed, and every line it was written on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rename {
    pub from: String,
    pub to: String,
    pub lines: Vec<usize>,
}

/// One place whose meaning changes in 2.0 and that no tool should rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub line: usize,
    pub message: String,
}

/// What migrating a program would do to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    /// The program with every rename applied. Equal to the input when there was
    /// nothing to rename.
    pub source: String,
    pub renames: Vec<Rename>,
    pub notes: Vec<Note>,
}

impl Migration {
    /// Whether 2.0 changes nothing about this program.
    pub fn is_empty(&self) -> bool {
        self.renames.is_empty() && self.notes.is_empty()
    }
}

/// Work out what 2.0 changes about `source`.
///
/// Fails only when the program does not lex or parse, because a file that is
/// not a program has nothing to migrate.
pub fn migrate(source: &str) -> Result<Migration, MiruError> {
    let (renamed, renames) = rename_reserved(source)?;
    let program = crate::parse_program(source)?;
    Ok(Migration {
        source: renamed,
        renames,
        notes: notes_for(&program),
    })
}

/// Where one identifier is written, in characters from the start of the file.
struct Occurrence {
    start: usize,
    len: usize,
    word: String,
    line: usize,
}

/// Rewrite every identifier that 2.0 reserves, and say what was rewritten.
///
/// **This works from token spans rather than from the AST.** Reprinting the AST
/// is what `miru fmt` does, and it would reformat the whole file, burying two
/// renamed words in a diff that touches every line. A person has to review this
/// change, so the change has to be reviewable: nothing moves except the names
/// that had to.
fn rename_reserved(source: &str) -> Result<(String, Vec<Rename>), MiruError> {
    let (tokens, spans) = Lexer::tokenize_with_spans(source)?;
    let chars: Vec<char> = source.chars().collect();
    let line_starts = line_starts(&chars);

    // Every name the file already uses, so a rename cannot land on one of them.
    let mut taken: HashSet<String> = HashSet::new();
    let mut occurrences: Vec<Occurrence> = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        match &token.kind {
            TokenKind::Ident(name) => {
                taken.insert(name.clone());
                if is_reserved(name) {
                    let Some((start, len)) = spans.tokens.get(index).copied() else {
                        continue;
                    };
                    occurrences.push(Occurrence {
                        start,
                        len,
                        word: name.clone(),
                        line: token.line,
                    });
                }
            }
            // A name inside `f"..."` is not a token of its own, so its span is
            // not in the table. It carries its own line and column for the sake
            // of error carets, and that is enough to find it here.
            TokenKind::FString(parts) => {
                for part in parts {
                    let FStringPart::Name { name, line, column } = part else {
                        continue;
                    };
                    taken.insert(name.clone());
                    if is_reserved(name) {
                        occurrences.push(Occurrence {
                            start: line_starts[line - 1] + column - 1,
                            len: name.chars().count(),
                            word: name.clone(),
                            line: *line,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    let replacements = choose_replacements(&occurrences, &mut taken);
    let renamed = apply(chars, &occurrences, &replacements);
    Ok((renamed, summarise(&occurrences, &replacements)))
}

fn is_reserved(name: &str) -> bool {
    RESERVED_IN_2.contains(&name)
}

/// The character offset each line starts at, indexed from zero.
fn line_starts(chars: &[char]) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (index, c) in chars.iter().enumerate() {
        if *c == '\n' {
            starts.push(index + 1);
        }
    }
    starts
}

/// Pick a new name for each reserved word the file uses.
///
/// A trailing underscore, and another for as long as the result is a name the
/// file already has. `match_` cannot itself be reserved, because no reserved
/// word ends in an underscore, so this always terminates on the first candidate
/// nothing has taken. Each choice is added to `taken` so two renames in one file
/// cannot arrive at the same name.
fn choose_replacements(
    occurrences: &[Occurrence],
    taken: &mut HashSet<String>,
) -> HashMap<String, String> {
    let mut replacements: HashMap<String, String> = HashMap::new();
    for occurrence in occurrences {
        if replacements.contains_key(&occurrence.word) {
            continue;
        }
        let mut candidate = format!("{}_", occurrence.word);
        while taken.contains(&candidate) {
            candidate.push('_');
        }
        taken.insert(candidate.clone());
        replacements.insert(occurrence.word.clone(), candidate);
    }
    replacements
}

/// Splice every rename into the source, working backwards so that an edit never
/// moves the offset of one that has not been made yet.
fn apply(
    chars: Vec<char>,
    occurrences: &[Occurrence],
    replacements: &HashMap<String, String>,
) -> String {
    let mut out = chars;
    let mut order: Vec<&Occurrence> = occurrences.iter().collect();
    order.sort_by(|a, b| b.start.cmp(&a.start));
    for occurrence in order {
        let Some(to) = replacements.get(&occurrence.word) else {
            continue;
        };
        out.splice(
            occurrence.start..occurrence.start + occurrence.len,
            to.chars(),
        );
    }
    out.into_iter().collect()
}

/// One entry per renamed word, holding the lines it appeared on, without
/// repeats and in order.
fn summarise(occurrences: &[Occurrence], replacements: &HashMap<String, String>) -> Vec<Rename> {
    let mut lines: HashMap<&str, Vec<usize>> = HashMap::new();
    for occurrence in occurrences {
        let entry = lines.entry(occurrence.word.as_str()).or_default();
        if !entry.contains(&occurrence.line) {
            entry.push(occurrence.line);
        }
    }
    let mut renames: Vec<Rename> = lines
        .into_iter()
        .map(|(word, mut lines)| {
            lines.sort_unstable();
            Rename {
                from: word.to_string(),
                to: replacements[word].clone(),
                lines,
            }
        })
        .collect();
    renames.sort_by(|a, b| a.from.cmp(&b.from));
    renames
}

/// The call sites 2.0 gives a different meaning, which a person has to look at.
///
/// Only two builtins are involved, and both were measured rather than reasoned
/// about: `slice` with a negative bound clamps it to 0 today and will count from
/// the end, and `index_of` and `find` return `-1` today and will return `nil`.
/// Everything else about negative indexing is an error today, so no working
/// program can be relying on it.
fn notes_for(program: &[Stmt]) -> Vec<Note> {
    // A file that defines its own `slice` is not calling the builtin, so it is
    // not affected and should not be told that it is.
    let mut bound: HashSet<String> = HashSet::new();
    for stmt in program {
        bound_in_stmt(stmt, &mut bound);
    }
    let mut notes = Vec::new();
    for stmt in program {
        notes_in_stmt(stmt, &bound, &mut notes);
    }
    notes.sort_by_key(|note| note.line);
    notes
}

/// Every name this statement introduces, at any depth.
fn bound_in_stmt(stmt: &Stmt, into: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Let { pattern, .. } => bound_in_pattern(pattern, into),
        StmtKind::For {
            name, value, body, ..
        } => {
            bound_in_pattern(name, into);
            if let Some(value) = value {
                bound_in_pattern(value, into);
            }
            for stmt in body {
                bound_in_stmt(stmt, into);
            }
        }
        StmtKind::Function { name, params, body } => {
            into.insert(name.clone());
            into.extend(params.iter().cloned());
            for stmt in body {
                bound_in_stmt(stmt, into);
            }
        }
        StmtKind::Import { alias, .. } => {
            into.insert(alias.clone());
        }
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            for stmt in then_branch.iter().chain(else_branch.iter().flatten()) {
                bound_in_stmt(stmt, into);
            }
        }
        StmtKind::While { body, .. } => {
            for stmt in body {
                bound_in_stmt(stmt, into);
            }
        }
        _ => {}
    }
}

fn bound_in_pattern(pattern: &Pattern, into: &mut HashSet<String>) {
    for name in pattern.names() {
        into.insert(name.to_string());
    }
}

fn notes_in_stmt(stmt: &Stmt, bound: &HashSet<String>, notes: &mut Vec<Note>) {
    match &stmt.kind {
        StmtKind::Let { value, .. } | StmtKind::Expr(value) => notes_in_expr(value, bound, notes),
        StmtKind::Return(value) => {
            if let Some(value) = value {
                notes_in_expr(value, bound, notes);
            }
        }
        StmtKind::Assign { target, value } | StmtKind::CompoundAssign { target, value, .. } => {
            notes_in_expr(target, bound, notes);
            notes_in_expr(value, bound, notes);
        }
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            notes_in_expr(condition, bound, notes);
            for stmt in then_branch.iter().chain(else_branch.iter().flatten()) {
                notes_in_stmt(stmt, bound, notes);
            }
        }
        StmtKind::While { condition, body } => {
            notes_in_expr(condition, bound, notes);
            for stmt in body {
                notes_in_stmt(stmt, bound, notes);
            }
        }
        StmtKind::For { iterable, body, .. } => {
            notes_in_expr(iterable, bound, notes);
            for stmt in body {
                notes_in_stmt(stmt, bound, notes);
            }
        }
        StmtKind::Function { body, .. } => {
            for stmt in body {
                notes_in_stmt(stmt, bound, notes);
            }
        }
        StmtKind::Import { .. } | StmtKind::Break | StmtKind::Continue => {}
    }
}

fn notes_in_expr(expr: &Expr, bound: &HashSet<String>, notes: &mut Vec<Note>) {
    match &expr.kind {
        ExprKind::Call { callee, arguments } => {
            if let ExprKind::Identifier(name) = &callee.kind {
                if !bound.contains(name) {
                    if let Some(message) = call_note(name, arguments) {
                        notes.push(Note {
                            line: expr.line,
                            message,
                        });
                    }
                }
            }
            notes_in_expr(callee, bound, notes);
            for argument in arguments {
                notes_in_expr(argument, bound, notes);
            }
        }
        ExprKind::Array(items) => {
            for item in items {
                notes_in_expr(item, bound, notes);
            }
        }
        ExprKind::Map(pairs) => {
            for (key, value) in pairs {
                notes_in_expr(key, bound, notes);
                notes_in_expr(value, bound, notes);
            }
        }
        ExprKind::Index { target, index } => {
            notes_in_expr(target, bound, notes);
            notes_in_expr(index, bound, notes);
        }
        ExprKind::Field { target, .. }
        | ExprKind::Unary {
            operand: target, ..
        } => notes_in_expr(target, bound, notes),
        ExprKind::Try(inner) => notes_in_expr(inner, bound, notes),
        ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
            notes_in_expr(left, bound, notes);
            notes_in_expr(right, bound, notes);
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            notes_in_expr(condition, bound, notes);
            notes_in_expr(then_value, bound, notes);
            notes_in_expr(else_value, bound, notes);
        }
        ExprKind::Function { body, .. } => {
            for stmt in body {
                notes_in_stmt(stmt, bound, notes);
            }
        }
        ExprKind::FString(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::Nil
        | ExprKind::Identifier(_) => {}
    }
}

/// What to say about a call to `name`, or nothing when 2.0 leaves it alone.
fn call_note(name: &str, arguments: &[Expr]) -> Option<String> {
    match name {
        "slice" => {
            let bounds = arguments.iter().skip(1);
            if bounds.clone().any(is_negative_literal) {
                Some(
                    "slice has a negative bound here: today it clamps to 0, in 2.0 it counts \
                     from the end"
                        .to_string(),
                )
            } else if bounds.clone().any(|bound| !is_non_negative_literal(bound)) {
                Some(
                    "slice: if a bound here can be negative, today it clamps to 0 and in 2.0 \
                     it counts from the end"
                        .to_string(),
                )
            } else {
                None
            }
        }
        "index_of" | "find" => Some(format!(
            "{name} gives nil rather than -1 when it finds nothing in 2.0, so a comparison \
             against -1 stops matching"
        )),
        _ => None,
    }
}

fn is_negative_literal(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::Unary {
            op: UnaryOp::Negate,
            operand,
        } if matches!(operand.kind, ExprKind::Int(_) | ExprKind::Float(_))
    )
}

fn is_non_negative_literal(expr: &Expr) -> bool {
    matches!(expr.kind, ExprKind::Int(_) | ExprKind::Float(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Migration {
        migrate(source).expect("the program parses")
    }

    #[test]
    fn a_program_that_needs_nothing_is_left_alone() {
        let source = "let total = 0\nfor i in range(3) {\n  total += i\n}\nprint(total)\n";
        let migration = run(source);
        assert!(migration.is_empty());
        assert_eq!(migration.source, source);
    }

    #[test]
    fn a_reserved_word_is_renamed_everywhere_it_is_written() {
        let migration = run("let match = 1\nprint(match + match)\n");
        assert_eq!(migration.source, "let match_ = 1\nprint(match_ + match_)\n");
        assert_eq!(
            migration.renames,
            vec![Rename {
                from: "match".to_string(),
                to: "match_".to_string(),
                lines: vec![1, 2],
            }]
        );
    }

    #[test]
    fn a_rename_does_not_land_on_a_name_the_file_already_has() {
        let migration = run("let match_ = 1\nlet match = 2\nprint(match_ + match)\n");
        assert_eq!(
            migration.source,
            "let match_ = 1\nlet match__ = 2\nprint(match_ + match__)\n"
        );
    }

    #[test]
    fn only_the_names_change_and_nothing_is_reformatted() {
        // The point of working from spans rather than reprinting the AST. The
        // odd spacing, the comment, and the blank line all survive, so the diff
        // a person reviews is the rename and nothing else.
        let source = "// a note\nlet   loop = 1\n\n\n  print( loop )\n";
        let migration = run(source);
        assert_eq!(
            migration.source,
            "// a note\nlet   loop_ = 1\n\n\n  print( loop_ )\n"
        );
    }

    #[test]
    fn a_name_inside_an_f_string_is_renamed_too() {
        // An f-string is one token, so the name inside it has no span of its
        // own. Missing it would leave a file that renames the declaration and
        // not the use, which is worse than not renaming at all.
        let migration = run("let case = 2\nprint(f\"it is {case} now\")\n");
        assert_eq!(
            migration.source,
            "let case_ = 2\nprint(f\"it is {case_} now\")\n"
        );
    }

    #[test]
    fn a_reserved_word_inside_a_string_is_not_a_name() {
        let source = "let m = {\"match\": 1}\nprint(m[\"match\"])\n";
        let migration = run(source);
        assert_eq!(migration.source, source);
        assert!(migration.renames.is_empty());
    }

    #[test]
    fn every_reserved_word_is_recognised() {
        for word in RESERVED_IN_2 {
            let migration = run(&format!("let {word} = 1\n"));
            assert_eq!(
                migration.source,
                format!("let {word}_ = 1\n"),
                "{word} was not renamed"
            );
        }
    }

    #[test]
    fn type_is_left_alone_because_it_is_still_a_builtin() {
        let source = "print(type(1))\n";
        assert_eq!(run(source).source, source);
    }

    #[test]
    fn a_negative_slice_bound_is_reported_rather_than_rewritten() {
        let migration = run("let a = [1, 2, 3]\nprint(slice(a, -2, 3))\n");
        assert_eq!(
            migration.source,
            "let a = [1, 2, 3]\nprint(slice(a, -2, 3))\n"
        );
        assert_eq!(migration.notes.len(), 1);
        assert_eq!(migration.notes[0].line, 2);
        assert!(migration.notes[0].message.contains("counts from the end"));
    }

    #[test]
    fn a_slice_bound_that_might_be_negative_is_reported_as_a_maybe() {
        let migration = run("fn f(a, n) {\n  return slice(a, n, 3)\n}\n");
        assert_eq!(migration.notes.len(), 1);
        assert!(migration.notes[0].message.contains("can be negative"));
    }

    #[test]
    fn a_slice_with_literal_bounds_is_not_reported() {
        let migration = run("let a = [1, 2, 3]\nprint(slice(a, 0, 2))\n");
        assert!(migration.notes.is_empty());
    }

    #[test]
    fn index_of_and_find_are_reported_wherever_they_are_called() {
        let migration =
            run("let a = [1]\nif index_of(a, 9) == -1 {\n  print(find(\"ab\", \"z\"))\n}\n");
        assert_eq!(migration.notes.len(), 2);
        assert_eq!(migration.notes[0].line, 2);
        assert!(migration.notes[0].message.starts_with("index_of gives nil"));
        assert_eq!(migration.notes[1].line, 3);
        assert!(migration.notes[1].message.starts_with("find gives nil"));
    }

    #[test]
    fn a_file_that_defines_its_own_slice_is_not_told_about_the_builtin() {
        let migration = run("fn slice(a, i) {\n  return a\n}\nprint(slice([1], -1))\n");
        assert!(
            migration.notes.is_empty(),
            "reported the builtin for a call that does not reach it: {:?}",
            migration.notes
        );
    }

    #[test]
    fn a_program_that_does_not_parse_says_so() {
        assert!(migrate("let = 1\n").is_err());
    }
}
