# Next steps

You now know all of MiruScriptX v0.6. Here is where to go from here.

## Try it without installing anything

The [playground](https://stiven-gjekaj.github.io/MiruScriptX/) runs MiruScriptX
in your browser. It is the same lexer, compiler, and virtual machine as the
`miru` command, built to WebAssembly, so anything that works there works on your
machine and the other way round. It has the example programs ready to load, a
Format button, and a tab showing the bytecode your program compiles to.

## Practice

Try writing a few small programs:

- Print the even numbers from 1 to 20.
- Write a function that reverses an array into a new array.
- Compute the greatest common divisor of two numbers with a `while` loop.

The programs in the [examples](../examples) folder are a good starting point.
Run them with `miru run examples/greet.miru`, and try
`miru run examples/greeter.miru` for one that reads your input.

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

The [roadmap](../docs/milestones.md) lists what is planned next, from making the
bytecode virtual machine the only engine to a browser playground.

---
Previous: [Errors](15-errors.md)
