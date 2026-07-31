# Builtins

These functions are always available. The set is small but grows with each
release.

## print(...)

Writes its arguments separated by single spaces, then a newline. Returns `nil`.

```
print("x is", 10, "and done")   // x is 10 and done
```

## eprint(...)

The same as `print`, to the error stream instead of the output stream.

```
print("the result")             // the result
eprint("something to note")     // something to note
```

Both appear on your screen, so the two look identical when you run a program
yourself. They are different when somebody redirects one:

```
miru run report.miru > results.txt
```

`print` goes into the file. `eprint` still reaches the terminal. That is the
point of having two: a program can say what it produced and separately say what
went oddly, without the second landing in the middle of the first.

## exit(code)

Stops the program and gives the code to whoever ran it. `0` means everything
worked and any other number means it did not. The code must be from 0 to 255.

```
fn check(n) {
  if n < 0 {
    eprint("n must not be negative")
    exit(2)
  }
  return n
}

print(check(5))    // 5
print(check(-1))   // stops here with code 2
```

A program that never calls `exit` gives `0` when it finishes and `1` if an error
stopped it, which is what it always did.

`try` cannot catch an `exit`. The program has stopped. See
[Handling errors](16-handling-errors.md).

## len(value)

Returns the number of items in an array, or the number of characters in a
string.

```
print(len([1, 2, 3]))   // 3
print(len("hello"))     // 5
```

## push(array, value)

Appends `value` to the end of `array`, changing it in place, and returns the
array.

```
let a = [1]
push(a, 2)
print(a)   // [1, 2]
```

## str(value)

Converts any value to its display string. Useful for building messages.

```
print("total: " + str(42))   // total: 42
```

## type(value)

Returns the name of a value's type, one of `int`, `float`, `bool`, `string`,
`array`, `map`, `function`, `nil`, or `error`.

```
print(type(3.14))   // float
```

## is_error(value)

Whether the value is an error caught by `try`. See
[Handling errors](16-handling-errors.md).

```
print(is_error(try 1 / 0))   // true
print(is_error(42))          // false
```

## range(end) or range(start, end)

Returns an array of integers. With one argument it counts from 0; with two it
counts from `start`. The `end` value is never included.

```
print(range(4))      // [0, 1, 2, 3]
print(range(2, 6))   // [2, 3, 4, 5]
```

## keys(map)

Returns an array of the map's keys, in sorted order.

```
print(keys({"b": 2, "a": 1}))   // ["a", "b"]
```

## values(map)

Returns an array of the map's values, in key order.

```
print(values({"b": 2, "a": 1}))   // [1, 2]
```

## has(map, key)

Reports whether the map contains a given string key.

```
let m = {"a": 1}
print(has(m, "a"))   // true
print(has(m, "z"))   // false
```

## remove(map, key)

Takes the key out of the map and gives back the value it held. A key that is
not there is not an error: you get `nil`.

```
let stock = {"apple": 3, "pear": 1}
print(remove(stock, "pear"))   // 1
print(stock)                   // {"apple": 3}
print(remove(stock, "plum"))   // nil
```

Because an absent key gives `nil` too, a key holding `nil` and a key that was
never there look the same afterwards. Ask with `has` first if you need to tell
them apart.

## String functions

- `upper(s)` and `lower(s)` change the case of every letter.
- `trim(s)` removes leading and trailing whitespace.
- `replace(s, from, to)` replaces every occurrence of `from` with `to`.
- `split(s, sep)` breaks a string into an array of pieces; an empty separator
  splits it into single characters.
- `join(array, sep)` joins an array's displayed elements with `sep`.
- `contains(seq, value)` reports whether a string holds a substring, or an array
  holds an element.
- `find(s, sub)` returns the character index of the first `sub`, or -1.

```
print(upper("hi"), lower("HI"))     // HI hi
print(trim("  hi  "))               // hi
print(replace("a.b.c", ".", "-"))   // a-b-c
print(split("a,b,c", ","))          // ["a", "b", "c"]
print(join(["a", "b", "c"], "-"))   // a-b-c
print(contains("hello", "ell"))     // true
print(find("hello", "l"))           // 2
```

## Array functions

- `pop(array)` removes and returns the last element.
- `index_of(array, value)` returns the index of the first match, or -1.
- `slice(seq, start, end)` returns the half-open slice of an array or string.
- `sort(array)` returns a sorted copy (all numbers or all strings).
- `reverse(seq)` returns a reversed copy of an array or string.

```
let xs = [3, 1, 2]
print(sort(xs))                // [1, 2, 3]
print(reverse(xs))             // [2, 1, 3]
print(slice(xs, 0, 2))         // [3, 1]
print(index_of([10, 20], 20))  // 1
```

## Math functions

- `abs(x)` is the absolute value.
- `min(...)` and `max(...)` take any number of numeric arguments.
- `floor(x)`, `ceil(x)`, and `round(x)` return integers.
- `sqrt(x)` is the square root (a float); `pow(base, exp)` raises to a power.

```
print(abs(-3), min(3, 1, 2), max(3, 1, 2))   // 3 1 3
print(floor(2.7), ceil(2.1), round(2.5))     // 2 3 3
print(sqrt(9), pow(2, 10))                    // 3.0 1024
```

## Conversion

- `int(x)` converts a float (truncating toward zero) or a numeric string to an
  integer.
- `float(x)` converts an integer or a numeric string to a float.

```
print(int("42"), int(2.9))      // 42 2
print(float("1.5"), float(3))   // 1.5 3.0
```

## input(prompt)

Reads one line from standard input and returns it as a string, without the
trailing newline. With a `prompt` argument, the prompt is written first (with no
newline). At end of input it returns `nil`.

```
let name = input("What is your name? ")
print("Hello,", name)
```

## Files and the command line

These four are what turn a program into a script: something you run from a
terminal, that reads a file, writes one, and takes arguments.

- `read_file(path)` gives the whole file as a string.
- `write_file(path, text)` writes the text, replacing whatever was there, and
  gives `nil`.
- `file_exists(path)` gives `true` if there is a file at the path.
- `args()` gives the arguments the program was given, as an array of strings.
  The program's own path is not one of them.

```
// upper.miru — read a file named on the command line and shout it
let names = args()
if len(names) == 0 {
  print("give me a file to read")
} else {
  let path = names[0]
  if file_exists(path) {
    print(upper(read_file(path)))
  } else {
    print("no file at", path)
  }
}
```

```
$ miru run upper.miru notes.txt
```

**A path is relative to where you are, not to where the script is.** If you run
`miru run scripts/tool.miru` and the program reads `data.txt`, it looks for
`data.txt` in the directory you ran the command from.

This is the opposite of `import`, which finds a module next to the file that
imports it. The two are different on purpose: a module is part of the program
and travels with it, while a data file belongs to whoever is running the
program.

Reading and writing fail with an error where there is no file system, such as in
the browser playground. `try` catches it:

```
let text = try read_file("data.txt")
if is_error(text) {
  print("could not read it:", text.message)
}
```

`file_exists` gives `false` there rather than failing, because the honest answer
to the question is then no.

## Higher-order functions

These apply a function across an array. The function can be a named function, a
closure, or another builtin.

- `map(array, f)` returns a new array of `f(x)` for each element.
- `filter(array, f)` returns a new array of the elements for which `f(x)` is
  truthy.
- `reduce(array, f, init)` folds the array from the left: it starts from `init`
  and combines each element with `f(acc, x)`, returning the final accumulator.

```
print(map([1, 2, 3], fn(x) { return x * 2 }))                 // [2, 4, 6]
print(filter([1, 2, 3, 4], fn(x) { return x % 2 == 0 }))      // [2, 4]
print(reduce([1, 2, 3, 4], fn(acc, x) { return acc + x }, 0)) // 10
```

The standard library stays small on purpose; see the
[roadmap](../docs/milestones.md) for what is planned next.

---
Previous: [Closures](12-closures.md) | Next: [Modules](14-modules.md)
