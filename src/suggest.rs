//! Choosing the name an error offers back when a program misspells one.
//!
//! A message that says only what is wrong leaves the reader to find the name
//! themselves. `undefined variable 'prnt'` is one letter from the answer, and
//! the error is the one place that knows both the name written and the names
//! available.
//!
//! Two rules keep this from becoming noise. At most one name is offered, since
//! a list of three gives the reader the search back. And a name is offered only
//! when it is close, because a message that guesses wildly is worse than one
//! that does not guess: it sends the reader somewhere before they have started
//! looking.

/// How far apart two names may be, as a fraction of the name that was written.
///
/// A third, rounded down, so a four-letter name accepts one edit and a
/// six-letter name accepts two. Measured against the 44 builtin names, this
/// accepts `prnt`, `strr`, `rang`, `eprnt`, `uppr`, and `sqrtt`, and declines
/// `xyzzy`, `missing`, and `totl`. It also declines `lenght`, which is three
/// edits from `len` because the builtin is not called `length`, and declining
/// is the right answer there.
const SHARE_OF_NAME: usize = 3;

/// The number of edits between two names, where a swap of two neighbouring
/// characters counts as one edit and not as two.
///
/// This is the optimal string alignment distance rather than plain Levenshtein,
/// and the difference is worth the five extra lines. A swap is the most
/// ordinary typing mistake there is, and plain Levenshtein charges two edits
/// for one, which the threshold above then declines. Measured against the real
/// builtin list, plain Levenshtein refused to suggest anything for `pirnt`,
/// `puhs`, `tpye`, `kesy`, `exti`, `spilt`, and `jion`. On the five field names
/// of an error it was worse than silent: it answered `flie` with `line`, where
/// counting the swap gives `file`.
///
/// Both names arrive already in lower case, from [`suggest`].
fn distance(a: &[char], b: &[char]) -> usize {
    let mut rows = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in rows[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let substitution = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (rows[i - 1][j] + 1)
                .min(rows[i][j - 1] + 1)
                .min(rows[i - 1][j - 1] + substitution);
            // The two characters are each other's neighbour the other way
            // round, so one swap explains both.
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = best;
        }
    }
    rows[a.len()][b.len()]
}

/// The one name from `candidates` worth offering for `written`, or `None`.
///
/// Case is ignored, so `LEN` finds `len`. The name that was written is never
/// offered back, which matters because a name can be present and still fail:
/// assigning to a builtin refuses, and answering `print` with `print` would say
/// nothing at all.
///
/// A tie is settled by name rather than by whichever candidate arrived first,
/// because the candidates come out of a hash map and their order is not the
/// same twice. An error message that varies from run to run is worse than
/// either answer.
pub fn suggest<'a>(written: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let typed: Vec<char> = written.to_lowercase().chars().collect();
    let limit = typed.len() / SHARE_OF_NAME;

    let mut best: Option<(&str, usize)> = None;
    for candidate in candidates {
        if candidate == written {
            continue;
        }
        let other: Vec<char> = candidate.to_lowercase().chars().collect();
        let apart = distance(&typed, &other);
        if apart > limit {
            continue;
        }
        let better = match best {
            None => true,
            Some((name, sofar)) => apart < sofar || (apart == sofar && candidate < name),
        };
        if better {
            best = Some((candidate, apart));
        }
    }
    best.map(|(name, _)| name.to_string())
}

/// `message`, with a suggestion added when one of `candidates` is close enough.
///
/// The suggestion goes on the end of the message rather than on a line of its
/// own. The stability guarantee fixes the shape of an error report and leaves
/// the words free, so a clearer message is permitted and a new line in the
/// report would not be.
pub fn with_suggestion<'a>(
    message: String,
    written: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> String {
    match suggest(written, candidates) {
        Some(name) => format!("{message}. Did you mean '{name}'?"),
        None => message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn among(written: &str, candidates: &[&str]) -> Option<String> {
        suggest(written, candidates.iter().copied())
    }

    #[test]
    fn a_name_one_edit_away_is_offered() {
        assert_eq!(
            among("prnt", &["print", "len", "push"]),
            Some("print".to_string())
        );
        assert_eq!(among("rang", &["range"]), Some("range".to_string()));
        assert_eq!(
            among("uppr", &["upper", "lower"]),
            Some("upper".to_string())
        );
    }

    #[test]
    fn a_swap_of_two_characters_is_one_edit() {
        // The case the distance exists for. Plain Levenshtein charges two for
        // each of these, which the threshold then declines.
        for (typed, wanted) in [
            ("pirnt", "print"),
            ("puhs", "push"),
            ("tpye", "type"),
            ("kesy", "keys"),
            ("exti", "exit"),
            ("spilt", "split"),
            ("jion", "join"),
        ] {
            assert_eq!(
                among(
                    typed,
                    &["print", "push", "type", "keys", "exit", "split", "join"]
                ),
                Some(wanted.to_string()),
                "{typed} should have found {wanted}"
            );
        }
    }

    #[test]
    fn a_name_that_is_not_close_is_not_guessed_at() {
        // The rule this is for: a message that guesses wildly sends the reader
        // somewhere before they have started looking.
        assert_eq!(among("xyzzy", &["print", "len", "type", "push"]), None);
        assert_eq!(among("missing", &["min", "max", "print"]), None);
        assert_eq!(among("totl", &["str", "print", "type"]), None);
        // Three edits from `len`, because the builtin is not called `length`.
        assert_eq!(among("lenght", &["len", "lower", "upper"]), None);
        // A single letter allows no edits at all, so nothing near it counts.
        assert_eq!(among("x", &["max", "abs"]), None);
    }

    #[test]
    fn case_is_ignored_but_the_name_written_is_never_offered() {
        assert_eq!(among("LEN", &["len", "keys"]), Some("len".to_string()));
        assert_eq!(among("Print", &["print"]), Some("print".to_string()));
        // Never the name written. A name can be present and still fail, since
        // assigning to a builtin refuses, and answering `print` with `print`
        // would say nothing at all.
        assert_eq!(among("print", &["print"]), None);
        assert_eq!(among("len", &["len", "keys"]), None);
        // What this rule does not do is stop a neighbour being offered instead.
        // `eprint` is one edit from `print`, so a caller that already knows the
        // name is present has to decline to ask rather than rely on this.
        assert_eq!(
            among("print", &["print", "eprint"]),
            Some("eprint".to_string())
        );
    }

    #[test]
    fn the_answer_does_not_depend_on_the_order_the_candidates_arrive_in() {
        // The candidates come out of a hash map. Two names the same distance
        // away would otherwise give a message that changes between runs.
        let forwards = among("lst", &["last", "list", "len"]);
        let backwards = among("lst", &["len", "list", "last"]);
        assert_eq!(forwards, backwards);
        assert_eq!(forwards, Some("last".to_string()));
    }

    #[test]
    fn a_suggestion_is_added_to_a_message_only_when_there_is_one() {
        assert_eq!(
            with_suggestion(
                "undefined variable 'prnt'".to_string(),
                "prnt",
                ["print", "len"]
            ),
            "undefined variable 'prnt'. Did you mean 'print'?"
        );
        assert_eq!(
            with_suggestion(
                "undefined variable 'xyzzy'".to_string(),
                "xyzzy",
                ["print", "len"]
            ),
            "undefined variable 'xyzzy'"
        );
    }
}
