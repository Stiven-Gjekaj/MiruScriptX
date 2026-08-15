# 1.11: a branch that gives a value

Two issues, and they are one idea: **choosing a value should be an expression.**
Today a branch can only act, so choosing between two values costs four lines and
a name that has to be mutable.

| Issue | What |
| ----- | ---- |
| [#49](https://github.com/Stiven-Gjekaj/MiruScriptX/issues/49) | `if` is an expression |
| [#48](https://github.com/Stiven-Gjekaj/MiruScriptX/issues/48) | `match`, also an expression |

Both are additive. Each is a parse error today, so section 2.1 of the
[guarantee](stability.md) covers the release and nothing here needs a 2.0.

This file is deleted when 1.11 ships, and its lessons move into
[milestones.md](milestones.md), as 1.8, 1.9, and 1.10 did.

## Why these two, and in this order

**#49 was decided before this plan existed**, in the comment on that issue: `if`
becomes an expression, and `? :` is declined. The reasoning is recorded there
and is not repeated here.

**#48 waited for that answer on purpose.** Its useful form is an expression
whose value is the arm taken, and section 2.1 freezes the meaning of syntax from
the release that adds it, so a `match` that only acts could never grow into one
that gives. It also waited for #42, which shipped in 1.10: with patterns in the
language, an arm can match a shape rather than only compare a constant.

Doing them in either other order does not work. Doing them apart wastes the
first one.

## Decisions settled before any code

### For `if` (#49)

- **The rule is scoped to position, not to blocks.** An `if` in value position
  gives the arm taken. A function body does not change. `fn f() { 1 }` returns
  `nil` today, measured, and the natural phrasing of this feature would silently
  make it return `1`, which section 5 of the guarantee puts in version 2.
- **`while` and `for` stay statements.** Neither has a value to give.
- **An `if` with no `else`, in value position, refuses at compile time.** Not
  `nil`. The compiler knows both facts it needs, so this is a static error with
  a caret, and `nil` would invent a value nobody wrote.
- **An `if` expression binds as a primary.** Its arms are brace-delimited, so
  there is no question where it ends: `if c { 1 } else { 2 } + 3` adds 3 to the
  result.
- **There is no bare block statement to worry about.** `{ 1 }` is a map literal
  and fails with `expected ':' after a map key`. The change reaches `if` arms
  and nothing else.

### For `match` (#48)

- **`match` is a contextual keyword, not a reserved one.** `let match = 1` and
  `fn match(x) { .. }` both run today, measured, so reserving the word breaks a
  valid 1.0 program. `switch` and `case` are the same. This is the single
  largest constraint on the feature and the reason it is not a morning's work.
- **An arm is written `->`.** `1 -> 2` is a parse error today, so the arrow is
  free.
- **A case is a literal, a bracketed pattern, or `else`.** Not an arbitrary
  expression, which would make the construct an if-chain with better syntax and
  give the compiler nothing to check. Not a bare name: `match x { y -> .. }`
  cannot be read at a glance as "bind `y`" or "compare with `y`", and a language
  that has to explain which is a language with a footgun. A bare name is refused,
  naming `else`.
- **`else` is the default arm**, reusing the word that already means otherwise
  rather than inventing `_` or `default`.
- **No fallthrough.** Stated rather than assumed, because C's default is a
  defect generator.
- **An unmatched value with no `else` arm refuses at runtime**, naming the
  value. Silence here could not be corrected later under section 2.3.
- **No `break` inside a `match`.** `break` keeps meaning "leave the loop", and a
  `match` inside a loop must not make that ambiguous. With no fallthrough there
  is nothing for it to do.
- **Duplicate literal cases are refused at compile time.** The compiler can see
  them, and a case that can never run is always a mistake.

## Order, and what may slip

**#48 is the one that may slip to 1.12**, and the plan is built so that it can.
If it does, 1.11 ships as #49 alone, which is a real release on its own: it is
the feature that removes the four-line dance from every program in `examples/`.

#42 carried the same warning in the 1.10 plan and landed. This is a warning, not
a forecast.

## The commits

Roughly forty-five, one idea each.

### Part one: `if` is an expression, closing #49

1. This plan file.
2. The parser reads `if` where an expression is expected.
3. The AST carries an `if` expression.
4. An arm's value is its trailing expression.
5. The compiler leaves one value per arm.
6. The statement form emits what it emitted before, asserted against the
   disassembly.
7. A missing `else` in value position refuses, with a caret on the `if`.
8. A function body's meaning is unchanged: `fn f() { 1 }` is still `nil`.
9. A trailing `if` statement still echoes `nil` in the REPL.
10. `while` and `for` are still statements, asserted.
11. An `if` expression binds as a primary.
12. Nesting: an `if` inside an arm, and an arm that is a call.
13. Every value position: `let`, an argument, an array element, a map value, a
    `return`, and the right side of a compound assignment.
14. Golden cases.
15. `miru fmt` round trips both forms.
16. An example uses it where it kept a mutable name.
17. Specification sections 3.2, 3.3, and 5, and the wiki's control-flow lesson.
18. The structure counts.

### Part two: `match`, closing #48

19. `match` is read as a contextual keyword, and `let match = 1` still runs.
20. The `->` token.
21. The parser reads a `match` and its arms.
22. A literal case.
23. A bracketed pattern case, reusing 1.10's `Pattern`.
24. `else` as the default arm.
25. A bare name as a case is refused, naming `else`.
26. Two `else` arms are refused.
27. The compiler emits the arms.
28. It is an expression, and its value is the arm taken.
29. An unmatched value with no `else` refuses at runtime, naming the value.
30. There is no fallthrough, asserted.
31. `break` inside a `match` inside a loop still leaves the loop.
32. A duplicate literal case is refused at compile time.
33. A `match` in statement position, for arms that only act.
34. Golden cases.
35. `miru fmt` round trips a `match`.
36. `tetris.miru` and `snake.miru` lose their `else if` chains.
37. `keys.miru` too, which is the clearest of the three.
38. Specification sections 3.2, 3.3, and 5, and a wiki lesson.
39. The structure counts.

### Part three: documentation, and cut 1.11.0

40. The changelog entry.
41. The guarantee records what becomes stable from 1.11.
42. The specification version, and the grammar summary.
43. The README feature list and the counts.
44. The single-page reference, regenerated.
45. The milestones section, carrying this file's lessons.
46. This file is deleted.
47. The version bump.
48. The release dispatch, and the tag it creates.
49. The playground deploy, checked against the live wasm.
