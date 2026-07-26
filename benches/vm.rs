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
//! Take the baseline immediately before the change, not from an earlier
//! session. These numbers drift by more than a typical change is worth
//! depending on what else the machine is doing, so only a paired before and
//! after taken minutes apart compares like with like.
//!
//! # Anything under five percent is not a result
//!
//! Rebuilding the crate moves these numbers on its own. Adding a public
//! function that no benchmark calls, and changing nothing else, measured as a
//! 3.9% improvement on `fib` and 3.2% on `loop_sum`, at p = 0.00 and with
//! confidence intervals as tight as any real change produced. A function that
//! is never called cannot make the interpreter faster. What moved was the
//! layout of the binary: where the dispatch loop falls relative to cache lines
//! and branch predictor entries shifts on any rebuild, and a tight interpreter
//! loop is unusually sensitive to it.
//!
//! So this harness has a floor of roughly four percent that looks exactly like
//! a real effect, statistics and all. Treat a change under five percent as
//! unmeasured rather than as small, and do not report it as an effect. Above
//! that, the further from the floor the more it is worth believing: the
//! twenty percent results in v0.5 are safely clear of it, an eight percent one
//! is worth a second build before it is trusted.
//!
//! The most useful check is not statistical. Ask what mechanism the change
//! gives for the number to move, and prefer evidence outside the timer when
//! there is any: a change that leaves the emitted opcode stream identical
//! cannot have changed what the VM does at runtime, whatever the benchmark
//! says. Re-running is legitimate for shaking out interference, but re-running
//! until a number flatters the change is how benchmarks come to mean nothing.
//!
//! # What the v0.5 optimization work came to
//!
//! Both columns were measured on the machine used for development, the first
//! before any of it and the second after all of it. Absolute numbers are only
//! comparable against other runs on the same machine; the point is the ratio.
//!
//! ```text
//!                  before        after     speedup
//! loop_sum        5.276 ms     1.218 ms      4.33x
//! globals         3.222 ms   722.13 us       4.46x
//! strings       605.68 us    233.88 us       2.59x
//! arrays        786.35 us    334.09 us       2.35x
//! fib             1.108 ms   644.59 us       1.72x
//! maps            1.775 ms     1.255 ms      1.41x
//! higher_order  638.72 us    624.78 us       1.02x
//! ```
//!
//! Five changes account for that: an integer fast path for binary operators,
//! decoding opcodes without checking them, deriving the running chunk once per
//! frame instead of once per instruction, resolving globals to slots at compile
//! time, and folding a constant right operand into the operator. Each was kept
//! because it moved a number by well more than the floor described above, and
//! because there was a mechanism to explain the movement.
//!
//! `higher_order` is the one that barely moved, and that is the honest reading
//! rather than a disappointment: it spends its time in `map`, `filter`, and
//! `reduce`, which allocate a result array and call back into a nested
//! bytecode loop per element. None of the five touched either cost. Making it
//! faster means going after the builtin bridge, which is a different piece of
//! work.
//!
//! `constants` is absent because it did not exist before this work; it was
//! added partway through, when constant folding turned out not to be exercised
//! by any of the others.
//!
//! # A design rejected on evidence, in v0.7
//!
//! v0.7 went after the bridge that paragraph names, and the roadmap had two
//! candidate designs for it. One was a trampoline, keeping the builtins native
//! and having them yield "call this, then resume me" to the single dispatch
//! loop. The other was to stop having a bridge at all, by writing `map`,
//! `filter`, and `reduce` in MiruScriptX itself as ordinary functions using
//! ordinary `Call` opcodes.
//!
//! The second one measures worse than what it would replace:
//!
//! ```text
//!                                        net    per element
//! native map, closure callback        104 ms         104 ns
//! map written in MiruScriptX          169 ms         169 ns
//! for x in xs { push(ys, f(x)) }      167 ms         167 ns
//! ```
//!
//! A million elements, whole-process wall clock, best of nine, with a
//! `range(1000000)` baseline of 25 ms subtracted. It trades one nested Rust
//! loop per element for a bytecode call per element, which is roughly a wash,
//! and then adds an interpreted loop and an interpreted `push` on top, which is
//! not. The last two rows are the same program written two ways and agree to
//! within one percent, which is the cross-check that the measurement is of the
//! thing it claims.
//!
//! So it is recorded here rather than left as an open option in the roadmap. A
//! design rejected with a number attached is worth more than one never tried,
//! and this is where somebody would come looking before proposing it again.
//!
//! The same measurements say `map` is not slow relative to the language: at
//! 104 ns per element it already beats the loop a user would write by hand at
//! 167 ns. `higher_order` did not move in v0.5 because half of its cost sits
//! outside the dispatch loop those five changes rewrote, which is a different
//! statement from the builtin being a bad deal.

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

/// The builtin bridge, measured against itself.
///
/// These two are a pair and mean nothing apart. `map` reaches a closure by
/// pushing a call frame and entering a nested bytecode loop, which is a real
/// Rust call per element. It reaches a native builtin by calling it in place,
/// with no frame and no nested loop. Everything else about the two programs is
/// the same, so the gap between them is very nearly the bridge alone.
///
/// Very nearly, not exactly: the two callbacks cannot do identical work, since
/// one is two opcodes and the other is a Rust match. That difference is a few
/// nanoseconds against a gap of roughly fifty, so it does not change the
/// reading, but it is the reason this is a close comparison rather than a
/// clean subtraction.
///
/// `higher_order` cannot show any of this. It runs `range`, `map`, `filter`,
/// and `reduce` together, so a change to the bridge arrives diluted by three
/// other things, which is how a real win reads as noise and gets discarded. It
/// stays as it is, unchanged, because it is the series that has to remain
/// comparable to the v0.5 table above.
const BRIDGE_CLOSURE: &str = "
let xs = range(20000)
len(map(xs, fn(x) { return x }))
";

const BRIDGE_BUILTIN: &str = "
let xs = range(20000)
len(map(xs, abs))
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
        ("bridge_closure", BRIDGE_CLOSURE),
        ("bridge_builtin", BRIDGE_BUILTIN),
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
