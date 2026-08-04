//! The random number generator behind `random`, `random_int`, and `seed`.
//!
//! Written here rather than taken from a crate. It is thirty lines of
//! arithmetic, and the language ships with two dependencies on purpose.
//!
//! The algorithm is SplitMix64: add a fixed odd constant to the state, then run
//! the sum through two multiply-and-shift rounds. It was chosen for what it
//! does not need. There is no state array to initialize, no warm-up, and
//! seeding is an assignment, so `seed(n)` means exactly one thing and a program
//! that sets a seed gets the same numbers on every machine that runs this
//! release.
//!
//! **Which numbers a seed produces is not part of the stability guarantee.**
//! Section 3 of `docs/stability.md` says so. A later 1.x can replace this.

/// A stream of numbers, decided by its seed.
///
/// `Copy` because it is one integer, which means the engine can hold it in a
/// field and hand it out without any of the ceremony the other capabilities
/// need.
#[derive(Clone, Copy)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// The seed used where the host has no clock to take one from.
    ///
    /// A constant rather than a refusal: a program that asks for a random
    /// number wants a number, and an embedder that supplies no clock has not
    /// said anything about randomness. The consequence is that such a host
    /// repeats its runs, which the specification states.
    ///
    /// The value is arbitrary. It is not zero only so that a stream nobody
    /// seeded is not the same as the stream from `seed(0)`.
    pub const WITHOUT_A_CLOCK: i64 = 0x4D69_7275;

    /// Odd, and close to 2^64 divided by the golden ratio. Being odd is what
    /// makes the addition walk through all 2^64 states before it repeats.
    const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

    pub fn seeded(seed: i64) -> Rng {
        Rng { state: seed as u64 }
    }

    /// The next 64 bits.
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(Rng::GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A float from 0 up to but not including 1.
    ///
    /// 53 bits, which is every bit of the mantissa and not one more. Taking
    /// more would ask the float to hold a value it has to round, and rounding
    /// up is how a generator that promises to stay below 1 returns 1.
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// An integer from `low` to `high`, both included. `low` must not be above
    /// `high`.
    ///
    /// The width is computed in `u64` with wrapping arithmetic because the
    /// range can be wider than an `i64` holds: from the smallest integer to the
    /// largest is 2^64 values, which is one more than a `u64` counts. That case
    /// wraps to 0 and is the whole range, so it takes the 64 bits unchanged.
    pub fn int_in(&mut self, low: i64, high: i64) -> i64 {
        let width = (high as u64).wrapping_sub(low as u64).wrapping_add(1);
        let offset = if width == 0 {
            self.next()
        } else {
            self.below(width)
        };
        (low as u64).wrapping_add(offset) as i64
    }

    /// A value from 0 up to but not including `bound`, which must not be 0.
    ///
    /// Plain `% bound` would be biased: 2^64 is not a multiple of most bounds,
    /// so the first few residues come up once more often than the rest. Over
    /// six sides that bias is about one part in 3 x 10^18 and no test would
    /// ever see it, which is exactly why it is worth removing here rather than
    /// leaving for somebody to find in a use that does care.
    ///
    /// `threshold` is 2^64 modulo `bound`, the count of values that spoil the
    /// division. Discarding a draw below it leaves a whole number of complete
    /// cycles, and `%` over those is uniform. The loop ends with probability 1
    /// and, for the worst bound, discards fewer than half of its draws.
    fn below(&mut self, bound: u64) -> u64 {
        let threshold = (u64::MAX - bound + 1) % bound;
        loop {
            let drawn = self.next();
            if drawn >= threshold {
                return drawn % bound;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A change detector, not a promise. The guarantee leaves the sequence
    /// free, so this test exists to make a change to the generator show up as a
    /// deliberate edit here rather than as a surprise somewhere downstream.
    #[test]
    fn the_sequence_for_a_given_seed_is_this_one() {
        let mut rng = Rng::seeded(1);
        let drawn: Vec<u64> = (0..4).map(|_| rng.next()).collect();
        assert_eq!(
            drawn,
            vec![
                10451216379200822465,
                13757245211066428519,
                17911839290282890590,
                8196980753821780235,
            ]
        );
    }

    /// The same seed gives the same stream. This is the property `seed(n)`
    /// exists for, and the one a program can depend on within a release.
    #[test]
    fn one_seed_gives_one_stream() {
        let mut a = Rng::seeded(7);
        let mut b = Rng::seeded(7);
        for _ in 0..64 {
            assert_eq!(a.next(), b.next());
            assert_eq!(a.unit(), b.unit());
        }
    }

    /// Different seeds give different streams. SplitMix64 finalizes the state
    /// rather than returning it, so seeds one apart do not give outputs one
    /// apart, which is the failure a weaker generator has here.
    #[test]
    fn neighbouring_seeds_do_not_give_neighbouring_numbers() {
        let mut a = Rng::seeded(1000);
        let mut b = Rng::seeded(1001);
        let (first, second) = (a.next(), b.next());
        assert_ne!(first, second);
        assert!(
            first.abs_diff(second) > 1_000_000,
            "the two streams start {} apart",
            first.abs_diff(second)
        );
    }

    /// `unit` stays inside its range over a long run, including at the top,
    /// where a generator that took 54 bits would round up to exactly 1.
    #[test]
    fn unit_stays_below_one() {
        let mut rng = Rng::seeded(99);
        let mut highest = 0.0f64;
        for _ in 0..100_000 {
            let value = rng.unit();
            assert!((0.0..1.0).contains(&value), "drew {value}");
            highest = highest.max(value);
        }
        assert!(highest > 0.999, "the top of the range was never approached");
    }

    /// The bounds of `int_in` are both reachable and neither is passed.
    #[test]
    fn int_in_covers_its_range_and_stays_inside_it() {
        let mut rng = Rng::seeded(3);
        let mut seen = [false; 6];
        for _ in 0..1000 {
            let value = rng.int_in(1, 6);
            assert!((1..=6).contains(&value), "drew {value}");
            seen[(value - 1) as usize] = true;
        }
        assert!(seen.iter().all(|hit| *hit), "some faces never came up");
    }

    /// A range of one value is that value, without consuming an unbounded
    /// number of draws in the rejection loop.
    #[test]
    fn a_range_of_one_gives_that_one() {
        let mut rng = Rng::seeded(5);
        for _ in 0..100 {
            assert_eq!(rng.int_in(42, 42), 42);
        }
    }

    /// The widest range is the case the wrapping arithmetic exists for. It is
    /// 2^64 values, one more than a u64 counts, so the width wraps to 0 and the
    /// draw is taken whole.
    #[test]
    fn the_widest_range_does_not_overflow() {
        let mut rng = Rng::seeded(11);
        let mut negative = false;
        let mut positive = false;
        for _ in 0..1000 {
            let value = rng.int_in(i64::MIN, i64::MAX);
            negative |= value < 0;
            positive |= value > 0;
        }
        assert!(negative && positive, "the full range was not covered");
    }

    /// A range that spans zero is the other case the unsigned arithmetic has to
    /// get right, and the one a program is far more likely to ask for.
    #[test]
    fn a_range_across_zero_is_uniform_enough_to_see() {
        let mut rng = Rng::seeded(13);
        let mut below = 0;
        for _ in 0..10_000 {
            let value = rng.int_in(-100, 99);
            assert!((-100..=99).contains(&value), "drew {value}");
            if value < 0 {
                below += 1;
            }
        }
        // Half of 10000 draws, give or take. A sign error in the wrapping
        // would put this at 0 or at 10000 rather than near 5000.
        assert!(
            (4500..=5500).contains(&below),
            "{below} of 10000 were below 0"
        );
    }
}
