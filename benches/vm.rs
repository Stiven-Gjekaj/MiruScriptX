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
//!
//! # Baseline
//!
//! Measured at the start of the v0.5 optimization work, on the machine used for
//! development. Absolute numbers are only comparable against other runs on the
//! same machine; the point is the relative movement each change produces.
//!
//! ```text
//! fib             1.108 ms
//! loop_sum        5.276 ms
//! arrays        786.35 us
//! higher_order  638.72 us
//! globals         3.222 ms
//! strings       605.68 us
//! maps            1.775 ms
//! ```

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
///
/// The values are deliberately kept bounded (they cycle with period seven)
/// rather than accumulating, so the loop measures the cost of reaching globals
/// and not the arithmetic, and cannot overflow however long it runs.
const GLOBALS: &str = "
let a = 0
let b = 0
let i = 0
while i < 10000 {
  a = b + 1
  b = a % 7
  i = i + 1
}
a + b
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

/// Arithmetic over constant subexpressions inside a loop, the pattern constant
/// folding exists for. Written the way a person would rather than pre-reduced:
/// a unit conversion spelled out for readability, recomputed every iteration.
const CONSTANTS: &str = "
let total = 0
let i = 0
while i < 20000 {
  total = total + i * (24 * 60 * 60) + (2 * 3 + 4)
  i = i + 1
}
total
";

fn workloads(c: &mut Criterion) {
    for (name, source) in [
        ("constants", CONSTANTS),
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
