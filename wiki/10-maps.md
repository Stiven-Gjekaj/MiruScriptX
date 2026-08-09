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

## Removing

Use `remove` to take a key out. It gives back the value that was there:

```
let scores = {"ken": 12, "mia": 9}
print(remove(scores, "ken"))   // 12
print(scores)                  // {"mia": 9}
```

Removing a key that is not there is not an error. You get `nil`:

```
print(remove(scores, "nobody"))   // nil
```

**Assigning `nil` does not remove a key.** It stores `nil` under it, and the key
stays:

```
let m = {"a": 1}
m["a"] = nil
print(len(m))        // 1, not 0
print(has(m, "a"))   // true
print(keys(m))       // ["a"]
```

This is worth knowing because assigning `nil` is the natural guess and it fails
quietly. Use `remove`.

One consequence: since removing an absent key also gives `nil`, a key holding
`nil` and a key that was never there give the same answer. Ask `has` before the
removal if you need to tell them apart.

## Checking and counting

Use `has` to test for a key and `len` for the number of entries:

```
let m = {"a": 1, "b": 2}
print(has(m, "a"))   // true
print(has(m, "z"))   // false
print(len(m))        // 2
```

## Going over a map

A `for` loop with two variables gives you the key and the value together:

```
let ages = {"Aiko": 3, "Ken": 5}
for name, age in ages {
  print(name + " is " + str(age))
}
```

Keys always come in sorted order, so the output is stable:

```
Aiko is 3
Ken is 5
```

**One variable is an error over a map.** `for x in ages` does not run, because a
key and a value are both fair guesses at what `x` would be and the language will
not pick for you:

```
for x in ages {                     // error: cannot iterate over a map with
  print(x)                          // one loop variable
}
```

Ask for the keys alone with `keys`, which is also how you hold them as a value
to sort, count, or pass on:

```
print(keys(ages))                   // ["Aiko", "Ken"]
print(values(ages))                 // [3, 5]
print(len(keys(ages)))              // 2

for name in keys(ages) {
  print(name)
}
```

Two variables work over an array too, and there the first one is the index:

```
for i, name in ["Aiko", "Ken"] {
  print(str(i) + ": " + name)       // 0: Aiko, then 1: Ken
}
```

Like a one-variable loop, this walks a copy. Adding a key inside the loop does
not give the loop another step.

---
Previous: [Arrays](09-arrays.md) | Next: [Functions](11-functions.md)
