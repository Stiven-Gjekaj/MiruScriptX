# 1.10: arrays and maps, said once

Four issues, chosen from the fourteen open after 1.9. They form one release
rather than four errands: **working with arrays and maps, said once.** Share or
copy, walk a map's entries, take a pair apart, count in either direction.

| Issue | What |
| ----- | ---- |
| [#39](https://github.com/Stiven-Gjekaj/MiruScriptX/issues/39) | `copy(v)` |
| [#41](https://github.com/Stiven-Gjekaj/MiruScriptX/issues/41) | `range` counts down and by a step |
| [#46](https://github.com/Stiven-Gjekaj/MiruScriptX/issues/46) | `for k, v in map` |
| [#42](https://github.com/Stiven-Gjekaj/MiruScriptX/issues/42) | `let [x, y] = pair` |

All four are additive. Each is an error today, so section 2.1 of the
[guarantee](stability.md) covers the release and nothing here needs a 2.0.

This file is deleted when 1.10 ships, and its lessons move into
[milestones.md](milestones.md), which is where 1.8 and 1.9 recorded theirs.

## Why these four

**#39 is the only correctness item on the whole open list.** Everything else is
awkwardness with a workaround; this one lets a program be quietly wrong.
`let b = a` shares, so `b[0] = 99` changes `a`, and `examples/tetris.miru`
already carries a defensive `slice(row, 0, width)` with a comment explaining
why. When an example needs a defensive copy and the idiom for it is "slice the
whole thing", the language is asking for a `copy`.

**#41 closes a hole rather than smoothing one.** `range(5, 0)` is `[]`, so a
descending `for` cannot be written at all. A `while` works, which is why this is
not called impossible — but a `for` that only counts up is a gap a reader meets
in the first hour.

**#42 and #46 are the two highest-frequency annoyances.** `tetris.miru` is
written in `cell[0]` and `cell[1]` throughout, and every map walk is `keys(m)`
and then an index, which allocates the whole key list and hashes each key twice.

## Decisions settled before any code

- **`copy` is shallow**, and named so it cannot be mistaken for deep. A deep
  form can be added later under section 2.3; a shallow `copy` that later starts
  copying deeply cannot. A naive deep copy also walks into the
  self-referential-value guards that v1.0 spent a milestone building.
  `map(grid, copy)` is the documented way to copy a grid.
- **A destructuring length mismatch refuses**, naming both lengths. Padding with
  `nil` turns a changed return value into a `nil` several lines away instead of
  an error on the line that is wrong.
- **`range` with a step of `0` refuses.** It is a loop that never ends. A step
  whose sign disagrees with the bounds gives an empty array rather than an
  error, because `range(5, 0)` is empty today and section 2.3 does not let that
  change.
- **`for x in map` — one variable over a map — stays an error.** Reasonable
  people expect keys and values with equal confidence, so picking either makes
  half of all readers wrong, and section 2.3 would freeze the coin flip.

## Order, and what may slip

Cheapest first, largest last, so the release survives losing its tail:

1. **#39 `copy`** — one builtin, no syntax.
2. **#41 `range`** — one optional argument, precedent set by `sort` in 1.6.
3. **#46 `for k, v`** — parser, compiler, and the loop opcodes.
4. **#42 destructuring** — the largest, and the one that may slip to 1.11.

#42 last is deliberate for a second reason: it is what makes
[#48](https://github.com/Stiven-Gjekaj/MiruScriptX/issues/48) worth building as
pattern matching rather than as an if-chain with better syntax, so getting it
wrong is expensive later.

## The commits

Roughly fifty, one idea each.

### Part one — `copy`, closing #39

1. This plan file.
2. `copy` for an array: a new array, same elements.
3. `copy` for a map.
4. `copy` of a string, a number, a boolean, and `nil`: the value itself.
5. `copy` of a caught error, and the arity refusal.
6. Golden cases: the copy is independent, and the shallow rule is visible.
7. Golden case: `map(grid, copy)` copies a grid, which is the documented idiom.
8. `tetris.miru` uses `copy` where it used `slice`, and the comment shrinks.
9. `BUILTIN_NAMES`, the three kind counts, and the regenerated grammar.
10. Specification section 8.3, and the wiki's data-types lesson.

### Part two — `range`, closing #41

11. A third argument, accepted and applied.
12. Counting down.
13. A step of `0` refuses.
14. A step whose sign disagrees gives an empty array.
15. The existing one- and two-argument forms are unchanged, asserted.
16. Golden cases for all of the above.
17. An example counts down where it used a `while`.
18. Specification section 8.4, and the wiki's loops lesson.

### Part three — `for k, v in map`, closing #46

19. The parser takes a second loop variable.
20. The AST carries it.
21. The compiler emits the pair for a map.
22. The VM iterates entries in key order, matching `keys`.
23. One variable over a map stays an error, with a message naming the fix.
24. `for i, v in array` gives the index alongside the element.
25. Changing the collection inside the loop is refused.
26. Golden cases.
27. `miru fmt` round trips both forms.
28. An example walks a map without `keys`.
29. Specification sections 7 and 8.6, and the wiki's maps lesson.

### Part four — destructuring, closing #42

30. The parser accepts `let [a, b] = expr`.
31. The AST carries the pattern.
32. The compiler binds each name.
33. A length mismatch refuses, naming both lengths.
34. Nesting: `let [[a, b], c]`.
35. `for [x, y] in pairs`.
36. Assignment as well as `let`, or a decision not to.
37. Golden cases.
38. `miru fmt` round trips a pattern.
39. `tetris.miru` and `snake.miru` take their pairs apart.
40. Specification sections 3.2 and 5, and the wiki's arrays lesson.

### Part five — documentation, and cut 1.10.0

41. The changelog entry.
42. The guarantee records what becomes stable from 1.10.
43. The specification version, and the grammar summary.
44. The README feature list and the counts.
45. The single-page reference, regenerated.
46. The milestones section, carrying this file's lessons.
47. This file is deleted.
48. The version bump.
49. The release dispatch, and the tag it creates.
50. The playground deploy, checked against the live wasm.
