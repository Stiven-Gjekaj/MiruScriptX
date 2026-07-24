//! Benchmarks for the MiruScriptX bytecode engine.
//!
//! Each case is run end to end (parse, compile, execute), so the numbers reflect
//! what a user experiences from `miru run` rather than the dispatch loop in
//! isolation.
//!
//! These exist to make optimization work honest. Record a baseline before a
//! change and compare after it:
//!
//! ```text
//! cargo bench -- --save-baseline before
//! # make the change
//! cargo bench -- --baseline before
//! ```
//!
//! An optimization that does not move these numbers is not an optimization, and
//! should be reverted rather than kept for the complexity it adds.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

/// Recursion and function-call overhead.
const FIB: &str = "
fn fib(n) {
  if n < 2 { return n }
  return fib(n - 1) + fib(n - 2)
}
fib(18)
";

/// A tight loop over locals and arithmetic.
const LOOP_SUM: &str = "
let i = 0
let sum = 0
while i < 20000 {
  sum = sum + i * 2
  i = i + 1
}
sum
";

/// Building and iterating arrays, exercising allocation and indexing.
const ARRAYS: &str = "
let items = []
let i = 0
while i < 2000 {
  push(items, i)
  i = i + 1
}
let total = 0
for x in items { total = total + x }
total
";

/// Closures and higher-order builtins.
const HIGHER_ORDER: &str = "
let xs = range(2000)
let doubled = map(xs, fn(x) { return x * 2 })
let evens = filter(doubled, fn(x) { return x % 4 == 0 })
reduce(evens, fn(a, b) { return a + b }, 0)
";

/// Repeated global reads and writes, which resolve by name.
const GLOBALS: &str = "
let a = 0
let b = 1
let i = 0
while i < 10000 {
  a = a + b
  b = a - b
  i = i + 1
}
a
";

/// String building, which allocates on every concatenation.
const STRINGS: &str = "
let s = \"\"
let i = 0
while i < 2000 {
  s = s + \"x\"
  i = i + 1
}
len(s)
";

/// Map insertion and lookup.
const MAPS: &str = "
let m = {}
let i = 0
while i < 2000 {
  m[str(i)] = i
  i = i + 1
}
let total = 0
for k in keys(m) { total = total + m[k] }
total
";

fn workloads(c: &mut Criterion) {
    for (name, source) in [
        ("fib", FIB),
        ("loop_sum", LOOP_SUM),
        ("arrays", ARRAYS),
        ("higher_order", HIGHER_ORDER),
        ("globals", GLOBALS),
        ("strings", STRINGS),
        ("maps", MAPS),
    ] {
        // Run once outside the timing loop so a benchmark that has silently
        // stopped computing the right thing fails here rather than reporting a
        // flattering number.
        miruscriptx::eval_source(source).expect("the benchmark program runs");

        c.bench_function(name, |b| {
            b.iter(|| miruscriptx::eval_source(black_box(source)).expect("runs"))
        });
    }
}

criterion_group!(benches, workloads);
criterion_main!(benches);
