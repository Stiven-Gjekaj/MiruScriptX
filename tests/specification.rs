//! Checks that hold `docs/specification.md` to the implementation.
//!
//! A specification that disagrees with the binary is worse than none: it is
//! believed. These tests cannot check the prose. They can check the two things
//! most likely to drift in silence: the list of builtins, and the table of
//! limits. Both are sets of names and numbers that a person keeps by hand.

use std::fs;

fn specification() -> String {
    fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/specification.md"
    ))
    .expect("the specification is in the repository")
}

#[test]
fn every_builtin_has_a_row_in_the_specification() {
    // The count in this project has been wrong twice in comments, in both
    // directions, because it was maintained by counting. Section 8 promises
    // behaviour for every builtin, so every builtin has to appear there.
    let text = specification();
    let missing: Vec<&str> = miruscriptx::builtins::BUILTIN_NAMES
        .iter()
        .copied()
        .filter(|name| !text.contains(&format!("`{name}(")))
        .collect();
    assert!(
        missing.is_empty(),
        "section 8 does not document these builtins: {missing:?}"
    );
}

#[test]
fn the_specification_states_the_number_of_builtins_correctly() {
    let text = specification();
    let count = miruscriptx::builtins::BUILTIN_NAMES.len();
    assert!(
        text.contains(&format!("There are {count} builtins")),
        "section 8 does not say there are {count} builtins"
    );
}

#[test]
fn the_limits_the_specification_states_are_the_limits_that_apply() {
    // Each of these was reached by a program during drafting. This test checks
    // the document still quotes the number, not that the limit still holds:
    // the goldens and the compiler own that.
    let text = specification();
    for (what, number) in [
        ("call depth", "10000"),
        ("call arguments", "255"),
        ("captured variables", "255"),
        ("array elements", "65535"),
        ("nesting for comparing and printing", "256"),
    ] {
        assert!(
            text.contains(number),
            "section 9 does not quote the {what} limit of {number}"
        );
    }
}

#[test]
fn the_source_nesting_limit_in_the_specification_is_the_one_the_parser_applies() {
    // Read from the constants rather than written out, because these are the
    // two limits set by how much stack the machine has rather than by a byte in
    // the bytecode. They are expected to move, and when they do the document
    // has to move with them.
    //
    // They are separate because they cost different amounts of stack: nesting
    // spends a parser frame per level and a chain spends none. Holding them to
    // one number refused chains that 1.0 ran everywhere.
    let text = specification();
    let nesting = miruscriptx::parser::Parser::MAX_NESTING;
    assert!(
        text.contains(&format!("| Nesting in the source text | {nesting} |")),
        "section 9 does not quote the source nesting limit of {nesting}"
    );
    let height = miruscriptx::parser::Parser::MAX_HEIGHT;
    assert!(
        text.contains(&format!("| Length of one expression | {height} |")),
        "section 9 does not quote the expression length limit of {height}"
    );
}
