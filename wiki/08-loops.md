# Loops

## for ... in

A `for` loop walks over the items of an array, binding each one to a name in
turn:

```
let colors = ["red", "green", "blue"]
for c in colors {
  print(c)
}
```

## Two names, for the position as well

Give the loop two names and the first one is the position:

```
for i, c in colors {
  print(str(i) + ": " + c)    // 0: red, then 1: green, then 2: blue
}
```

That saves keeping a counter of your own, which is a line to write and a line to
forget to increment. Over a map the two names are the key and the value instead;
[Maps](10-maps.md) covers that.

Brackets go in either position, for a loop over pairs:

```
for [x, y] in [[0, 1], [2, 3]] {
  print(x + y)                // 1, then 5
}
```

[Arrays](09-arrays.md) covers what the brackets do and what happens when the
lengths disagree.

## Counting with range

`range` builds an array of integers, which is the usual way to count. With one
argument it starts at 0; with two it starts at the first:

```
for i in range(3) {
  print(i)          // 0, then 1, then 2
}

for i in range(2, 5) {
  print(i)          // 2, then 3, then 4
}
```

The end value is never included, so `range(0, n)` gives you exactly `n` numbers.

## A worked example

Sum the numbers from 1 to 10:

```
let total = 0
for i in range(1, 11) {
  total = total + i
}
print(total)   // 55
```

## Loop control: break and continue

Use `break` to stop a loop early, and `continue` to skip to the next iteration:

```
for n in range(1, 10) {
  if n == 5 { break }        // stop the loop entirely
  if n % 2 == 0 { continue } // skip the even numbers
  print(n)                   // 1, then 3
}
```

Both work in `while` loops too, and they always affect the nearest enclosing
loop.

## Counting down, and by more than one

`range` takes a third number, the step:

```
for i in range(0, 10, 2) {
  print(i)          // 0 2 4 6 8
}

for i in range(5, 0, -1) {
  print(i)          // 5 4 3 2 1
}
```

A negative step counts down. The end is still left out, the same as counting
up, so `range(5, 0, -1)` stops at 1.

A step of `0` is an error, because that loop would never end. A step pointing the
wrong way gives nothing at all rather than an error, so `range(0, 10, -1)` is
an empty array and the loop body simply does not run.

---
Previous: [Control flow](07-control-flow.md) | Next: [Arrays](09-arrays.md)
