# Maps

A map (also called a dictionary) holds a set of values, each stored under a
string key. Maps are written with curly braces.

## Creating and reading

```
let person = {"name": "Aiko", "age": 3}
print(person["name"])   // Aiko
print(person["age"])    // 3
```

Keys are strings. Reading a key that is not present gives `nil`:

```
print(person["email"])   // nil
```

## Adding and updating

Assign to a key to insert it, or to change a value that is already there:

```
let scores = {}
scores["ken"] = 10
scores["ken"] = 12
print(scores)   // {"ken": 12}
```

## Checking and counting

Use `has` to test for a key and `len` for the number of entries:

```
let m = {"a": 1, "b": 2}
print(has(m, "a"))   // true
print(has(m, "z"))   // false
print(len(m))        // 2
```

## Going over a map

Get the keys or values as arrays with `keys` and `values`, then loop:

```
let ages = {"Aiko": 3, "Ken": 5}
for name in keys(ages) {
  print(name + " is " + str(ages[name]))
}
```

Keys always come back in sorted order, so the output is stable:

```
Aiko is 3
Ken is 5
```

---
Previous: [Arrays](09-arrays.md) | Next: [Functions](11-functions.md)
