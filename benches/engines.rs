//! Benchmarks comparing MiruScriptX's two execution engines: the tree-walking
//! interpreter and the bytecode virtual machine.
//!
//! Each program is run end to end (parse, then evaluate or compile and run) on
//! both engines, so the numbers reflect what a user actually experiences from
//! `miru run` versus `miru run --vm`. Run them with `cargo bench`.

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

fn compare_engines(c: &mut Criterion) {
    for (name, source) in [
        ("fib", FIB),
        ("loop_sum", LOOP_SUM),
        ("arrays", ARRAYS),
        ("higher_order", HIGHER_ORDER),
    ] {
        // Both engines must agree before timing them, so a benchmark can never
        // report a speedup that came from doing less work.
        let tree = miruscriptx::eval_source(source).expect("the tree walker runs");
        let vm = miruscriptx::eval_source_vm(source).expect("the VM runs");
        assert_eq!(tree.repr(), vm.repr(), "engines disagree on {name}");

        let mut group = c.benchmark_group(name);
        group.bench_function("tree_walker", |b| {
            b.iter(|| miruscriptx::eval_source(black_box(source)).expect("runs"))
        });
        group.bench_function("bytecode_vm", |b| {
            b.iter(|| miruscriptx::eval_source_vm(black_box(source)).expect("runs"))
        });
        group.finish();
    }
}

criterion_group!(benches, compare_engines);
criterion_main!(benches);
