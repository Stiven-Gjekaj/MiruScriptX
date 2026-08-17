# Next steps

You now know all of MiruScriptX 2.0. Here is where to go from here.

## Try it without installing anything

The [playground](https://stiven-gjekaj.github.io/MiruScriptX/) runs MiruScriptX
in your browser. It is the same lexer, compiler, and virtual machine as the
`miru` command, built to WebAssembly, so anything that works there works on your
machine and the other way round. It has the example programs ready to load, a
Format button, and a tab showing the bytecode your program compiles to.

**Share** copies a link that carries the program you wrote. It goes in the part
of the address after the `#`, which a browser never sends to a server, so
sharing a program sends it to the person you give the link to and to nobody
else.

What it cannot do is anything that needs a file, because a browser has no file
system to give it: `import`, `read_file`, and `write_file` all report there
rather than pretending. `file_exists` answers `false` and `args` gives an empty
array, since those are the honest answers when there is no file system and no
command line. `input` reads nothing and `read_key` refuses, because a page has
no keyboard to read from the way a terminal does.

`eprint` and `exit` do work there. A page has no process to end, but it can say
what a program asked for, so `eprint` output appears marked as its own stream
and a non-zero code is reported under the output. That is an honest answer,
unlike reading a file, where the honest answer is that it cannot.

## Practice

Try writing a few small programs:

- Print the even numbers from 1 to 20.
- Write a function that reverses an array into a new array.
- Compute the greatest common divisor of two numbers with a `while` loop.

The programs in the [examples](../examples) folder are a good starting point.
Run them with `miru run examples/greet.miru`, try
`miru run examples/greeter.miru` for one that reads your input, and
`miru run examples/shop.miru` for a pair of files that work together.

## Look things up

For a single searchable page covering everything in this wiki, see the
[language reference](../docs/language-reference.md).

## See how it works

Curious how the language is built? The
[architecture guide](../docs/architecture.md) walks through the lexer, parser,
compiler, and virtual machine. You can also see the bytecode for any program
yourself:

```
miru disasm hello.miru
```

## What is coming

The [roadmap](../docs/milestones.md) lists what has shipped, milestone by
milestone, and what is planned next.

## Coming from version 1

**1.12 was the last version 1.** Version 2 turned sixteen ordinary words into
keywords, so a version 1 program that uses one as a name no longer parses:

```
async  await  case   const   default  defer  enum   finally
is     loop   match  pub     struct   until  use    yield
```

`miru migrate` reads a version 1 program on purpose, so it still works from a
version 2 binary and you can upgrade first:

```
miru migrate hello.miru        // say what changes, and change nothing
miru migrate -w hello.miru     // rename what it can, and report the rest
```

It renames those words wherever you used them, including inside an `f"..."`
string, and it changes nothing else about the file: your spacing, your comments
and your blank lines are exactly where you left them, so the only thing in the
diff is the rename.

**It will not rewrite a call whose meaning changes**, because deciding those
needs to know what a value will be while the program runs. There are two, and
it reports both with a line number:

- `slice` with a bound that can be negative. In version 1 a negative bound
  counted as `0`, so `slice(a, -2, 3)` was the first three elements. Now it
  counts from the end.
- `index_of` and `find`, which gave `-1` when they found nothing and now give
  `nil`. A program that writes `if index_of(a, x) == -1` needs
  `if index_of(a, x) == nil` instead.

A program that uses none of those is told so and left alone.
[Migrating to version 2](../docs/migrating-to-2.md) is the full guide.

---
Previous: [Writing a game](17-writing-a-game.md)
