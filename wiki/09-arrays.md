# Arrays

An array is an ordered list of values.

## Creating and reading

```
let fruits = ["apple", "banana", "cherry"]
print(fruits[0])   // apple
print(fruits[2])   // cherry
```

Indexing is zero-based. Reading past the end, or with a negative index, is an
error.

## Changing an element

Assign to an index to replace an element in place:

```
let nums = [1, 2, 3]
nums[1] = 20
print(nums)   // [1, 20, 3]
```

## Length

`len` returns how many items an array has (and it also works on strings):

```
print(len([10, 20, 30]))   // 3
print(len("hello"))        // 5
```

## Growing an array

`push` adds an item to the end and returns the array:

```
let stack = []
push(stack, 1)
push(stack, 2)
print(stack)   // [1, 2]
```

`insert` puts one somewhere else. It counts from zero, so `insert(a, 0, v)`
puts `v` at the front:

```
let queue = ["b", "c"]
insert(queue, 0, "a")
print(queue)   // ["a", "b", "c"]
```

The position can be anywhere from the front to the end. `insert(a, len(a), v)`
is the same as `push(a, v)`. Further than that is an error rather than a quiet
append, because an index past the end usually means the arithmetic that
produced it went wrong.

## Joining two arrays

`+` on two arrays gives a **new** array with the second after the first:

```
print([1, 2] + [3])   // [1, 2, 3]
```

Neither of the originals changes. That is the difference from `push`: `push`
grows an array you already have, and `+` is an expression that leaves its
operands alone.

```
let front = [1]
let back = [2]
let both = front + back
push(both, 3)
print(front)   // [1] -- untouched
print(both)    // [1, 2, 3]
```

This is the shape a lot of small programs want. Putting a new item at the front
and dropping the last one is one line:

```
body = [head] + slice(body, 0, len(body) - 1)
```

## Iterating

Combine arrays with a `for` loop to process every item:

```
let names = ["Aiko", "Ken"]
for n in names {
  print("Hello, " + n)
}
```

## Taking one apart

A function returns one value, so anything that returns two returns an array. Put
brackets on the left of a `let` and each element gets its own name:

```
let [x, y] = [3, 4]
print(x)              // 3
print(y)              // 4
```

That is worth reaching for whenever the elements mean different things. `size[0]`
and `size[1]` are a puzzle with a correct answer; `width` and `height` are not:

```
let [width, height] = term_size()
```

**The lengths have to match.** Two names need an array of exactly two:

```
let [x, y] = [1, 2, 3]    // error: cannot take apart an array of 3 with a
                          // pattern of 2
```

That is deliberate. If a short array quietly filled the extra names with `nil`,
a function that started returning three values would show up as a `nil` several
lines later, instead of as an error on the line that is wrong.

A pattern can hold a pattern, for an array inside an array:

```
let [[a, b], c] = [[1, 2], 3]
print(a, b, c)        // 1 2 3
```

A `for` loop takes the same brackets, which is where this earns its keep:

```
let cells = [[0, 1], [2, 3]]
for [x, y] in cells {
  print(x + y)        // 1, then 5
}
```

Two things it does not do, both on purpose. It does not take a map apart — use
[a two-variable loop](10-maps.md) for that. And it works with `let` and `for`,
not with assignment: `[a, b] = pair` is an error.

## Sharing, and copying

An array is a **reference**. Two names can reach the same array, and a change
through one is seen through the other:

```
let a = [1, 2]
let b = a
b[0] = 99
print(a)      // [99, 2]
```

That is often what you want — it is how a function can change an array you pass
it. When it is not, `copy` gives you an array that shares nothing:

```
let b = copy(a)
b[0] = 99
print(a)      // [1, 2]
```

**`copy` is shallow.** A grid is an array of arrays, and copying it gives a new
outer array holding the same rows, so writing into a row is still seen by both.
Copy each row to get a grid of your own:

```
let mine = map(grid, copy)
```

Maps work the same way, and `copy` copies one the same way.

---
Previous: [Loops](08-loops.md) | Next: [Maps](10-maps.md)
