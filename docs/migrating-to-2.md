# Migrating to version 2

Version 1 ran from 1.0 to 1.12 and kept one promise the whole way: a program
that was correct on 1.0 was correct on every later 1.x. Version 2 breaks it, on
purpose, in a small and listed way.

This page is what to do about it. The
[stability guarantee](stability.md), section 7, says why.

---

## The short version

```
miru migrate -w yourprogram.miru
```

Run that on each file. It renames what it can, and prints a line for each place
it will not touch. Then read those lines, which is the part no tool can do for
you.

**It works from the version 2 binary.** The program that needs migrating is
exactly the program the version 2 lexer refuses, so `migrate` reads a version 1
program on purpose. Upgrading first and migrating second is fine.

---

## What changes

There are three kinds, and only the third needs thought.

### 1. Sixteen words became keywords

```
async  await  case   const   default  defer  enum   finally
is     loop   match  pub     struct   until  use    yield
```

A program using one as a name no longer parses:

```
miru: error (line 1, column 5): 'match' is a keyword and cannot be a name.
'miru migrate -w' renames it, and reads a version 1 program to do it.
```

**`miru migrate -w` fixes this completely.** It renames each one to the same
word with an underscore (`match` becomes `match_`), everywhere it is written,
including inside an `f"..."` string. It changes nothing else about the file:
your spacing, comments and blank lines stay where they are, so the diff you
review is the rename and nothing else.

Fifteen of the sixteen still mean nothing. They were reserved together because
reserving one word per major version means a major version for every feature
that wants one.

### 2. Four builtins refuse where they used to answer

Each of these was a place the language did something the rest of it would not.
A program that never hit one of these cases is unaffected.

| Expression | Version 1 | Version 2 |
| ---------- | --------- | --------- |
| `pow(-8.0, 0.5)` | `nan` | an error |
| `int(float("1e300"))` | `9223372036854775807` | an error |
| `map([], nil)` | `[]` | an error |
| `pad_left("", 20000000)` | a 20 MB string | an error |

If your program relied on one of these, it was relying on a wrong answer. The
`nan` case is the one worth checking for: a `nan` travels through every
operation that touches it, so a program could be carrying one from a `pow` far
away and only notice now.

### 3. A negative index counts from the end

This is the one that needs reading rather than running.

```
let a = [1, 2, 3, 4]
a[-1]              // was an error, is now 4
slice(a, -2, 4)    // was [1, 2, 3, 4], is now [3, 4]
slice(a, 1, -1)    // was [], is now [2, 3]
insert(a, -1, 9)   // was an error, is now [1, 2, 3, 9, 4]
```

`a[-1]` and `insert(a, -1, v)` were errors, so no working program used them.
**`slice` is the one that silently changes**, because a negative bound used to
clamp to `0` and now counts back. `miru migrate` reports every `slice` call
whose bounds are not plain non-negative numbers, so you can look at each one.

Most of them will be fine. The shape to look for is a bound that can go
negative when a collection is short:

```
slice(items, 0, len(items) - 1)   // fine when items is never empty
```

### 3a. `index_of` and `find` give `nil`

This is part of the same change rather than a separate one, and it is the one
most likely to be in your code:

```
let i = index_of(items, wanted)
if i == -1 {                       // never true now
  print("not found")
}
```

Both used to answer `-1` for "not here". That was safe only while no index could
be negative. Now `-1` is the last element, so `items[index_of(items, missing)]`
would answer with the last element rather than failing, which is a wrong answer
replacing an error. `nil` is not an index, so the mistake still stops.

**The fix is to compare against `nil`:**

```
let i = index_of(items, wanted)
if i == nil {
  print("not found")
}
```

`miru migrate` reports every call to `index_of` and `find`. It cannot rewrite
them, because whether the result is compared, stored, or used as an index is not
something a tool can decide.

---

## What is new, and optional

None of this is needed to migrate. It is here because it is what the release is
for.

**`match`**, with guards and several cases per arm:

```
match pressed {
  "left" if fits(x - 1) { x = x - 1 }
  "q", "ctrl+c", "escape" { running = false }
  else { }
}
```

**A default on a parameter**, worked out afresh on each call that omits it:

```
fn greet(name, greeting = "Hello") { return greeting + ", " + name }
```

**A `...rest` parameter**, which collects whatever is left into an array:

```
fn log_all(prefix, ...rest) { for m in rest { print(prefix + m) } }
```

---

## What did not change

Everything else. The evaluation order, the truth rules, equality and order, the
shape of an error report, the module rules, the termination promise, and all 67
builtin names are the statements they were in 1.0.

A major version is permission, not an instruction.

---

## If something else broke

The list above is the whole of it. If a program of yours changed behaviour in a
way this page does not explain, that is a defect rather than a decision, and it
is worth reporting.
