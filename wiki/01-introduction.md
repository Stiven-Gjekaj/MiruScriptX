# Introduction

MiruScriptX is a complete, general-purpose scripting language. If you have written
a little JavaScript, Python, or Lua, it will feel familiar: dynamic types, a
clean syntax, first-class functions, and arrays.

Programs live in files with a `.miru` extension. They are compiled to bytecode
and run on a stack virtual machine, all written in Rust from scratch.

## Hello, world

```
print("Hello, world!")
```

Output:

```
Hello, world!
```

That is a complete program. `print` is a builtin that writes its arguments
followed by a newline.

## What you will learn

These pages are meant to be read in order, like short lessons. By the end you
will know the whole language: values, variables, operators, control flow,
functions and closures, arrays, every builtin, and how to split a program
across more than one file.

If you prefer a single page you can search, see the
[language reference](../docs/language-reference.md).

---
Next: [Getting started](02-getting-started.md)
