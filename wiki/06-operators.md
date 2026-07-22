# Operators

## Arithmetic

```
print(7 + 2)    // 9
print(7 - 2)    // 5
print(7 * 2)    // 14
print(7 / 2)    // 3     integer division truncates
print(7 % 2)    // 1     remainder
print(7.0 / 2)  // 3.5   a float operand gives float division
```

Two integers produce an integer. If either operand is a float, the result is a
float. Dividing or taking the remainder by zero is an error.

## Comparison

Comparisons produce a boolean:

```
print(1 < 2)      // true
print(2 <= 2)     // true
print(3 > 5)      // false
print(1 == 1.0)   // true    numbers compare across int and float
print("a" == "a") // true
print(1 != 2)     // true
```

Strings compare in dictionary order, so `"apple" < "banana"` is true.

## Logical

`&&` (and), `||` (or), and `!` (not). Both `&&` and `||` short-circuit, meaning
the right side is only evaluated when it is needed:

```
print(true && false) // false
print(false || true) // true
print(!true)         // false
```

## String concatenation

`+` joins two strings. To join a string with a number, convert the number first
with `str`:

```
let n = 3
print("you have " + str(n) + " messages")
```

## Precedence

From loosest to tightest: `||`, then `&&`, then the comparisons, then `+` and
`-`, then `*`, `/`, and `%`, then the prefix operators `-` and `!`, then calls
and indexing. Use parentheses when in doubt:

```
print(2 + 3 * 4)   // 14
print((2 + 3) * 4) // 20
```

---
Previous: [Data types](05-data-types.md) | Next: [Control flow](07-control-flow.md)
