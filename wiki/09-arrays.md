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

---
Previous: [Loops](08-loops.md) | Next: [Maps](10-maps.md)
