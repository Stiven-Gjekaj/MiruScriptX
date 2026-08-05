# Editor support

Syntax highlighting for `.miru` files.

## Visual Studio Code

```sh
ln -s "$(pwd)/editors/vscode" ~/.vscode/extensions/miruscriptx
```

Restart VS Code and open any `.miru` file. Use `%USERPROFILE%\.vscode\extensions`
on Windows.

## Sublime Text

Sublime reads the same grammar. Copy
`editors/vscode/syntaxes/miruscriptx.tmLanguage.json` into your `Packages/User`
directory and rename it to end in `.sublime-syntax` or install it through
PackageDev, which converts it.

## What is coloured, and what is not

Six classes, and nothing else:

| Class | What |
| ----- | ---- |
| comment | `//` to the end of the line |
| string | a string literal, with its escapes marked inside it |
| number | an integer or a float literal |
| keyword | `fn` `let` `return` `if` `else` `while` `for` `in` `break` `continue` `import` `as` `try` |
| literal | `true` `false` `nil` |
| builtin | any of the 53 names in `BUILTIN_NAMES` |

**Punctuation and an ordinary identifier are left plain.** That is not an
omission. `class_of` in `playground/src/lib.rs` made the same decision for the
browser, and its comment gives the reason: colouring punctuation makes code
busier to read rather than clearer. The two agree so that the language looks the
same in an editor and in the playground.

A name after a dot is a field rather than a builtin, so `c.len` is plain while
`len(c)` is coloured.

## The builtin list is generated

`scripts/build_grammar.sh` reads `BUILTIN_NAMES` from `src/builtins.rs` and
writes the alternation into the grammar. **Run it after adding a builtin** and
commit what it changes. That array has moved in five consecutive releases, so a
list typed by hand into a grammar is a list that is wrong within a month.

The names go in longest first, because a regular expression alternation is tried
left to right and a shorter name that prefixes a longer one would match first
and leave the rest of the word plain.

## Checking it

There is **no automated test**. A grammar is a thing you look at.

Open `editors/highlight-test.miru`, which has one line for each rule that a
grammar written from regular expressions gets wrong, with a comment saying what
should be coloured. It deliberately does not run: it holds an unknown escape and
an unterminated string, because those are the two cases where a bad grammar
loses the colour for the rest of the file.

Then open the programs in `examples/`, which do run.
