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
//! # The floor is not four percent everywhere
//!
//! That four percent was established on the machine this project is developed
//! on. It does not hold on a shared or containerized host, and v0.7 found out
//! the hard way. Six runs of one unchanged binary, no rebuild between them,
//! gave 1.69, 2.37, 2.44, 2.34, 2.45, and 2.47 ms on the same case: five of
//! them within six percent of each other and one thirty percent below the rest.
//!
//! Criterion's own interval is not the warning here. Each of those runs
//! reported a confidence interval under one percent wide, so every one of them
//! looked precise while disagreeing with its neighbours by thirty. A tight
//! interval measures how consistent a run was with itself, not how repeatable
//! it is.
//!
//! Two rules follow, and v0.7 broke both before it learned them.
//!
//! **Run a comparison more than once.** A single before-and-after pair can
//! report thirty percent that is not there. One did, was believed for an hour,
//! and had to be retracted in the commit that claimed it.
//!
//! **Quote a best of several rather than one run.** The fastest run of a set is
//! the one least disturbed by whatever else the host was doing, and on this
//! harness the slow runs cluster while the fast ones scatter, so the minimum is
//! the more stable end. Four runs was enough here to bring two builds into
//! clusters four to six percent wide, tight enough to tell a twenty-seven
//! percent difference from nothing.
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
//! # What the v0.7 work came to
//!
//! Best of four runs each, the two builds measured back to back, net of the
//! `bridge_setup` case, in nanoseconds per element:
//!
//! ```text
//!                        before      after
//! map with a closure     128.4      117.1      -8.8%
//! map with a builtin      84.6      107.4     +26.9%
//! ```
//!
//! That is the honest result and it is not the one the milestone set out to
//! get. Replacing the builtin bridge with a trampoline made a closure callback
//! slightly faster and a native one meaningfully slower. The nested Rust call
//! per element, which the whole plan was aimed at removing, was never the
//! dominant cost: a state machine yields a step per element where the loop it
//! replaced was a plain Rust `for`, and that costs more than the call did.
//!
//! It was kept anyway, because it also retires `MAX_HOST_CALL_DEPTH`. Recursion
//! through `map` used to fail at 64 levels while direct recursion got 10,000;
//! now there is one limit. That is a correctness fix, not a speed one, and it is
//! the whole return on this milestone.
//!
//! Two smaller results worth keeping:
//!
//! - The 48.9 ns gap this milestone was aimed at, measured in v0.7.13, was never
//!   a removable cost. It was the difference between two callback mechanisms,
//!   and replacing one of them with another of similar cost is what happened.
//! - A change can be a simplification and not an optimization. Dropping the
//!   callee from `Step` removed a field and measured as nothing; it was kept for
//!   the former reason after being proposed for the latter.
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
/// The two bridge cases without the `map`, so the pair can be read in absolute
/// terms rather than only as a gap.
///
/// A gap says how much one arm costs over the other. It does not say what
/// either costs, and a milestone that wants to claim it halved something needs
/// that. Subtracting this leaves what `map` itself does: the snapshot of its
/// input, an `Rc` bump of the callback per element, a `Vec` per element, and
/// the result array.
///
/// The reading, over twenty thousand elements and net of this case:
///
/// ```text
/// map with a closure   123.1 ns per element
/// map with a builtin    74.2 ns per element
/// the difference        48.9 ns per element
/// ```
///
/// Adding this third case to the file, and changing nothing else, moved the
/// other two by -5.3% and -7.5% while the gap between them moved -1.3%. That is
/// the layout floor described at the top of this file, caught in the act, and
/// it says which number to trust: **the gap is the robust measurement and the
/// absolute figures are not**. Both arms shift together on a rebuild, so a
/// difference taken within one run survives what either column alone does not.
/// Quote an absolute only alongside the run it came from.
///
/// The gap is also a *lower* bound on the bridge rather than an exact figure,
/// because the builtin arm still pays `call_native` and `abs` where the closure
/// arm pays a frame push and a nested loop. The bridge costs at least this
/// much, not at most.
///
/// # Where the builtin arm's 74 ns goes
///
/// Measured by *doubling* rather than removing: run a component ten extra times
/// per element, take the difference, divide by ten. Doubling keeps behavior
/// identical, so the whole suite still passes while the ablation is in place,
/// and the tenfold multiplier lifts the effect clear of the layout floor that
/// would otherwise swallow it. The edits were reverted; only the findings kept.
///
/// ```text
/// per-element Vec (allocate and free)   10.8 ns per element   +128% at 10x
/// input snapshot (amortized)             6.3 ns per element    +75% at 10x
/// callback clone                        not resolvable
/// the rest, roughly 57 ns                not separated
/// ```
///
/// **The callback ablation failed, and saying so is the point.** Ten extra
/// clones per element measured 21% *faster* than none. Adding work cannot make
/// a program faster, so that is codegen and layout, not a result. The mechanism
/// says why it could never have been resolved this way: for a `Value::Builtin`
/// a clone copies a string reference and a function pointer with no allocation
/// and no refcount, and for a `Value::Closure` it is one non-atomic increment.
/// Either is a nanosecond or so, which sits under the floor at this multiplier.
/// It is left unmeasured rather than reported at a number the method cannot
/// support.
///
/// That the same 10x multiplier resolved the first two at +128% and +75%, and
/// failed on the third, is the useful shape: it worked where the effect was
/// real and large, and refused where it was not.
///
/// The unseparated remainder is `call_native` saving and restoring the position,
/// swapping the input reader out and back, the dynamic call into `abs`, the push
/// onto the result array, and the loop itself. None of it is what this milestone
/// changes, so it was not worth further ablation.
const BRIDGE_SETUP: &str = "
let xs = range(20000)
len(xs)
";

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
        ("bridge_setup", BRIDGE_SETUP),
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
