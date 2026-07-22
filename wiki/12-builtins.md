# Builtins

These functions are always available. In v0.1 the set is deliberately small.

## print(...)

Writes its arguments separated by single spaces, then a newline. Returns `nil`.

```
print("x is", 10, "and done")   // x is 10 and done
```

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
`array`, `function`, or `nil`.

```
print(type(3.14))   // float
```

## range(end) or range(start, end)

Returns an array of integers. With one argument it counts from 0; with two it
counts from `start`. The `end` value is never included.

```
print(range(4))      // [0, 1, 2, 3]
print(range(2, 6))   // [2, 3, 4, 5]
```

More builtins are planned; see the [roadmap](../docs/milestones.md).

---
Previous: [Closures](11-closures.md) | Next: [Next steps](13-next-steps.md)
