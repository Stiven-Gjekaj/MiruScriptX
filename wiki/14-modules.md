# Modules

A program can be more than one file. `import` runs another file and binds
everything that file defines to a name of your choosing.

## Importing a file

```
import "./prices.miru" as prices

print(prices.tax_percent)   // 8
```

The path is relative to the file doing the importing, not to the directory you
ran `miru` from. `examples/shop.miru` imports `"./prices.miru"` and finds
`examples/prices.miru` wherever you run it from.

The alias is required. `import "./prices.miru"` on its own is a syntax error,
because there would be no name to reach the module through.

## What a module gives you

Every name defined at the top level of a file is reachable through the alias.
There is nothing to write to make a name public.

`prices.miru`:

```
let tax_percent = 8

fn with_tax(amount) {
  return amount + amount * tax_percent / 100
}
```

`shop.miru`:

```
import "./prices.miru" as prices

print(prices.with_tax(1300))   // 1404
```

The other side of that is that a module cannot keep a helper to itself. Every
top-level name is visible to whoever imports the file.

## Names belong to their file

Two files can use the same name without either one noticing:

```
import "./prices.miru" as prices

// `subtotal` is a function over in the module and a number here.
let subtotal = prices.subtotal(cart)
print(subtotal)
```

Before modules there was one table of names for the whole program, so this was
impossible to write. Now each file has its own.

## A module is a map

The alias holds an ordinary map, so anything that works on a map works here:

```
import "./prices.miru" as prices

print(keys(prices))            // ["subtotal", "tax_percent", "with_tax"]
print(prices["tax_percent"])   // 8
```

The two ways of reaching a name differ in what they do when the name is not
there. `prices.nope` is an error that points at the line, and `prices["nope"]`
is `nil`, the same as any other map lookup. Reach for the dot when a typo
should stop the program.

## Each file runs once

The first `import` of a file runs it from top to bottom. Later imports of the
same file, from anywhere in the program, get the same result without running it
again.

`shared.miru`:

```
print("loading")
let n = 7
```

If three different files import `shared.miru`, `loading` prints once. Two
spellings of one path, `./shared.miru` and `./sub/../shared.miru`, are the same
file and share the one result.

## Imports cannot form a cycle

If `aa.miru` imports `bb.miru` and `bb.miru` imports `aa.miru`, there is no
order in which either one can finish. MiruScriptX says so instead of looping:

```
error (./aa.miru, line 1, column 8): import cycle: ./bb.miru -> ./aa.miru -> ./bb.miru
```

The chain is printed in the order the files were reached, so you can see the
loop rather than guess at it.

## Where an import can appear

Only at the top level of a file. Inside an `if`, a loop, or a function it is an
error:

```
fn load() {
  import "./prices.miru" as prices
}
```

```
error (line 2, column 10): import must appear at the top level of a file
```

Imports are resolved before the file containing them starts running, so the
alias is in scope everywhere in the file, including above the `import` line.
Write them at the top regardless: it is where a reader looks to see what a file
depends on.

## Errors from inside a module

An error in an imported file names that file, so you know which one to open:

```
error (./bad.miru, line 1, column 11): undefined variable 'rate'
```

## In the playground

The browser [playground](https://stiven-gjekaj.github.io/MiruScriptX/) has no
file system, so `import` there reports that the program was not loaded from a
file. Everything else in this lesson behaves the same in both places.

---
Previous: [Builtins](13-builtins.md) | Next: [Errors](15-errors.md)
