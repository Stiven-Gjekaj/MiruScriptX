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

## insert(array, index, value)

Puts `value` at `index`, moving everything from there onwards along. Changes the
array in place and returns it.

```
let a = [2, 3]
insert(a, 0, 1)
print(a)   // [1, 2, 3]
```

`push` can only add to the end; this is how you add anywhere else, and adding to
the front is much the commonest.

The index counts from zero and can be anything from `0` to `len(array)`.
`insert(a, len(a), v)` appends, exactly like `push`. Anything past that is an
error, not a quiet append: an index beyond the end almost always means the sum
that produced it was wrong, and you would rather hear about it.

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
- `starts_with(s, prefix)` and `ends_with(s, suffix)` report whether a string
  begins or ends with another. An empty needle gives `true`, and one longer
  than the string gives `false`.

```
print(upper("hi"), lower("HI"))     // HI hi
print(trim("  hi  "))               // hi
print(replace("a.b.c", ".", "-"))   // a-b-c
print(split("a,b,c", ","))          // ["a", "b", "c"]
print(join(["a", "b", "c"], "-"))   // a-b-c
print(contains("hello", "ell"))     // true
print(find("hello", "l"))           // 2
print(starts_with("hello.miru", "hello"))   // true
print(ends_with("hello.miru", ".miru"))     // true
```

## Array functions

- `pop(array)` removes and returns the last element.
- `index_of(array, value)` returns the index of the first match, or -1.
- `slice(seq, start, end)` returns the half-open slice of an array or string.
- `sort(array)` returns a sorted copy (all numbers or all strings).
  `sort(array, key)` sorts by something else; see below.
- `reverse(seq)` returns a reversed copy of an array or string.

```
let xs = [3, 1, 2]
print(sort(xs))                // [1, 2, 3]
print(reverse(xs))             // [2, 1, 3]
print(slice(xs, 0, 2))         // [3, 1]
print(index_of([10, 20], 20))  // 1
```

### Sorting by something other than the value

`sort(array)` puts numbers or strings in order. To sort anything else, give it a
second argument: a function that says what to sort each element **by**.

```
let people = [
  {"name": "Mai", "age": 31},
  {"name": "Aiko", "age": 24},
  {"name": "Ken", "age": 45},
]

for p in sort(people, fn(x) { return x.age }) {
  print(p.age, p.name)
}
```

The function is asked for a key, not for a comparison. It receives one element
and returns the value to order that element by. Those keys follow the same rule
the elements do: all numbers, or all strings.

Any function works, including a builtin:

```
print(sort(["bbb", "a", "cc"], len))   // ["a", "cc", "bbb"]
```

**For decreasing order, reverse it:**

```
print(reverse(sort(scores, fn(x) { return x })))
```

**The sort is stable**, which means two elements with the same key keep the
order they were already in. That is what makes sorting by two things work: sort
by the less important one first, then by the more important one.

```
let by_name = sort(people, fn(p) { return p.name })
let result = sort(by_name, fn(p) { return p.age })
// same age, and Aiko comes before Ken
```

## Math functions

- `abs(x)` is the absolute value.
- `min(...)` and `max(...)` take any number of numeric arguments.
- `floor(x)`, `ceil(x)`, and `round(x)` return integers.
- `sqrt(x)` is the square root (a float); `pow(base, exp)` raises to a power.
- `sum(array)` adds the numbers in an array and `product(array)` multiplies
  them. An empty array gives `0` and `1`, so that adding up the pieces of a
  split array still gives the total.

```
print(abs(-3), min(3, 1, 2), max(3, 1, 2))   // 3 1 3
print(floor(2.7), ceil(2.1), round(2.5))     // 2 3 3
print(sqrt(9), pow(2, 10))                    // 3.0 1024
print(sum([1, 2, 3]), product([2, 3, 4]))     // 6 24
print(sum([]), product([]))                   // 0 1
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

## read_key()

`input()` waits for you to finish a line and press Enter. `read_key()` gives you
the key the moment it is pressed.

```
print("Press a key, or q to stop.")
while true {
  let key = read_key()
  if key == nil { break }
  if key == "q" { break }
  if key == "ctrl+c" { break }
  print("you pressed", key)
}
```

A key that makes a character gives you that character: `"a"`, `"A"`, `" "`.
Everything else gives a name: `"up"`, `"down"`, `"left"`, `"right"`, `"enter"`,
`"tab"`, `"escape"`, `"backspace"`, `"delete"`, `"home"`, `"end"`, `"pageup"`,
`"pagedown"`, `"insert"`, `"f1"` through `"f12"`, and `"ctrl+a"` through
`"ctrl+z"`. A key with no name gives `"unknown"`, so a loop can ignore it.

Tab gives `"tab"` and not `"ctrl+i"`, even though your terminal sends the same
thing for both. Enter and Backspace work the same way.

### Two things to know before you use it

**Control-C will not stop your program.** While you are reading keys, the
terminal hands Control-C to you as `"ctrl+c"` instead of stopping anything. That
is why the loop above checks for it. **A program that does not check for it
cannot be stopped from the keyboard**, and you will have to close the window.
Check for it in every loop you write.

**The terminal goes back to normal by itself** when your program ends, whether
it finished, failed, or called `exit`. You do not have to put it back.

Somewhere without a keyboard, such as the browser playground, `read_key()` fails
and `try` catches it:

```
let k = try read_key()
if is_error(k) {
  print("no keyboard here:", k.message)
}
```

## key_ready()

`read_key()` waits. That is usually what you want, and for anything that moves
it is exactly what you do not: **your program only gets to do something when
somebody presses a key.** A ball cannot fall while you sit still, because your
program is not running — it is waiting inside `read_key()`.

`key_ready()` tells you whether `read_key()` would answer straight away, so you
can look without committing to a wait:

```
while true {
  while key_ready() {
    let k = read_key()
    if k == nil { return }
    if k == "q" || k == "ctrl+c" { return }
    turn(k)
  }
  fall()          // happens whether or not anybody pressed anything
  draw()
  sleep(50)
}
```

The inner loop takes **everything** pressed since the last picture, rather than
one key. Somebody who presses three keys quickly gets all three handled now,
instead of one now and the others over the next two pictures.

**It answers `true` when the keys have run out**, which sounds wrong and is the
useful part. It is telling you the read will not make you wait — and a read at
the end does not, it gives `nil` at once. That is what lets the loop above
notice the end and stop. If it said `false` there instead, the loop would go
round forever waiting for a key that is never coming.

So `key_ready()` means *"will reading make me wait?"*, not *"is a key down?"*.

Where there is no keyboard at all it fails rather than saying `false`, because
"nothing is pressed" would be a lie about a keyboard that does not exist.

## now()

`now()` gives the number of milliseconds since the start of 1970, as an integer.
That date is where computers count time from, and the number itself is rarely
what you want. The difference between two of them is:

```
let started = now()
let total = 0
for n in range(1, 1000000) {
  total = total + n
}
print("took", now() - started, "milliseconds")
```

This is the first builtin whose answer is not decided by what you wrote.
`upper("hi")` is `"HI"` today and next year. `now()` is different every time you
call it, which is the whole point, and also the reason a program that has to
print the same thing twice should not use it.

The clock comes from whoever is running your program. `miru` has one, and so
does the browser playground. Somewhere without one, `now()` fails and `try`
catches it:

```
let t = try now()
if is_error(t) {
  print("no clock here:", t.message)
}
```

**Do not use it to measure short things.** The clock can be corrected while your
program runs, which makes it jump backwards, and a difference you took across
that moment comes out negative. For anything that has to be right about a
duration, say what your program does when the answer is below zero.

## sleep(ms)

`sleep(ms)` does nothing for that many milliseconds, and then your program
carries on.

```
for n in range(3, 0) {
  print(n)
  sleep(1000)
}
print("go")
```

**This is what makes a loop run at a speed you chose** rather than as fast as
the machine happens to be. A loop with nothing to slow it down runs millions of
times a second, which pins a processor at full speed and makes anything you draw
flash past. Anything that moves on a screen wants to wait between one picture
and the next:

```
while true {
  draw()
  sleep(50)      // twenty pictures a second
}
```

A negative number is an error rather than a wait of no time. Nobody means to
wait for less than nothing, so it is a mistake somewhere earlier — usually a
subtraction that came out the wrong way round — and it is better to hear about
it than to have your loop quietly run flat out. `sleep(0)` is fine, because that
same subtraction reaches zero honestly.

**The browser playground cannot do this**, and it is the one thing there that
has a clock and still refuses. A page draws between one piece of work and the
next, so a page that waited would stop drawing: your program would freeze the
tab and then show its last picture, rather than animating. `try` catches it:

```
let waited = try sleep(50)
if is_error(waited) {
  print("cannot pause here:", waited.message)
}
```

That is why anything that moves is a program for a terminal, and why you will
not find one in the playground.

## clear(), move_to(column, row), and the cursor

`print` puts a line below the last one. That is right for a program that reports
what it did and wrong for one that draws, because a picture printed every frame
gives you a column of pictures scrolling past rather than one that moves.

`clear()` empties the screen and puts the cursor back at the top left, so the
next picture lands where the last one was:

```
let at = 0
while at < 20 {
  clear()
  print(repeat_dots(at) + "#")
  at = at + 1
  sleep(100)
}
```

**Build the whole picture as one string and print it once.** Printing it a row
at a time works, and you can see it happening: the terminal draws each row as it
arrives, and the eye catches the sweep down the screen.

`move_to(column, row)` puts the cursor somewhere without clearing. It counts
from zero, so `move_to(0, 0)` is the top left — the same corner `grid[0][0]`
means.

`hide_cursor()` stops the terminal drawing the cursor, which otherwise sits
blinking wherever your last character went. `show_cursor()` puts it back, and
**so does the end of your program**, however it ends. You do not have to pair
them, just as you do not have to put the terminal back after `read_key()`.

`term_size()` gives `[columns, rows]`.

### When your output is not a terminal

Send a program's output to a file and there is no screen to draw on. `clear()`,
`move_to()`, and the two cursor calls **do nothing** in that case, which is what
you want: your file holds the text you printed instead of a mess of control
characters.

`term_size()` is the exception and **fails**, because there is no honest answer.
A file is not eighty columns wide; it has no columns. Giving you a number would
make you draw a picture sized for a screen that is not there.

That is why the games in `examples/` pick their own size rather than asking. It
also means you can pipe keys into one and compare what it prints, which is how
they are tested.

## random(), random_int(low, high), and seed(n)

`random()` gives a number from 0 up to but not including 1. `random_int(low,
high)` gives a whole number, and both ends count:

```
print(random_int(1, 6))       // a die
print(random_int(0, 1))       // a coin, as 0 or 1
print(random() < 0.3)         // true about three times in ten
```

To pick from an array, ask for an index:

```
let colours = ["red", "green", "blue"]
print(colours[random_int(0, len(colours) - 1)])
```

`len(colours) - 1` is there because indexes start at 0 and both ends of
`random_int` count. Getting this wrong by one is the usual mistake, and the
symptom is an error about an index out of range on about one run in three.

### Making a run repeat

Every run of your program gives different numbers, because the generator starts
from the clock. `seed(n)` starts it from `n` instead, and the same seed always
gives the same numbers:

```
seed(1)
print(random_int(1, 100), random_int(1, 100))   // the same two numbers
seed(1)                                         // every time you run this
print(random_int(1, 100), random_int(1, 100))
```

This is how a program that uses chance is tested. Every example in this
repository that draws a number calls `seed` first, which is what lets its test
assert the exact output.

**Do not save a seed and expect it to work forever.** A later version of
MiruScriptX can change the generator, and then the same seed gives different
numbers. What is promised is the range, and that one seed repeats within one
version.

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
