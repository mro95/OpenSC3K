//! `cRZRandom`, the game's random number generator, as built into the Windows DLLs.
//! See `docs/sim/random.md`. The Loki Linux build ships an older variant with a different
//! output step; the Windows one is the reference.

/// Multiplier and increment of the integer LCG.
const LCG_MUL: u64 = 0x41C6_4E6D;
const LCG_ADD: u64 = 0x3039;
/// Multiplier of the separate LCG behind the floating-point calls.
const DOUBLE_MUL: u32 = 0x278D_DE6D;
/// Seed the floating-point LCG falls back to when its state is zero.
const DOUBLE_RESEED: u32 = 0x1234_5678;
/// Scale of the floating-point output, stored in SIMDIRT.DLL at 0x10020F08. It is 2^-32
/// plus 7 ulp.
const DOUBLE_SCALE: u64 = 0x3DF0_0000_0000_0007;
/// Ranges at or above this use rejection instead of a modulo.
const LARGE_RANGE: u32 = 0x00FF_FFFF;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Random {
    state: u32,
    double_state: u32,
    /// Outermost calls, for comparing against the original (`tools/diffcheck`). `None` until
    /// [`Random::start_trace`], so a build that has the feature on through cargo's feature
    /// unification does not log in the game.
    #[cfg(feature = "trace")]
    trace: Option<Vec<Call>>,
    /// How many traced methods are on the stack; only depth 0 is logged.
    #[cfg(feature = "trace")]
    depth: u32,
}

/// The public methods, numbered for the call trace (`tools/diffcheck/rust.py` uses the same
/// numbers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Op {
    Seed = 0,
    NextU32 = 1,
    Uniform = 2,
    Range = 3,
    GaussianFast = 4,
    RangeMinOfTwo = 5,
    Double = 6,
    DoubleRange = 7,
}

/// One outermost call: the method, its arguments (integers as `u32`, doubles as their bits)
/// and both states on entry. Logged only with the `trace` feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Call {
    pub op: Op,
    pub a: u64,
    pub b: u64,
    pub state: u32,
    pub double_state: u32,
}

impl Random {
    /// `cRZRandom::cRZRandom(unsigned)`. The original treats a seed of `0xFFFFFFFF` as "seed
    /// from the clock" (`timeGetTime`); callers pick the clock seed themselves here.
    pub fn new(seed: u32) -> Random {
        Random {
            state: seed,
            double_state: seed,
            #[cfg(feature = "trace")]
            trace: None,
            #[cfg(feature = "trace")]
            depth: 0,
        }
    }

    /// Starts logging calls.
    #[cfg(feature = "trace")]
    pub fn start_trace(&mut self) {
        self.trace.get_or_insert_with(Vec::new);
    }

    /// Takes the calls logged so far and stops logging.
    #[cfg(feature = "trace")]
    pub fn take_trace(&mut self) -> Vec<Call> {
        self.trace.take().unwrap_or_default()
    }

    /// Runs `f`, logging the call first when it is not nested in another method. Without the
    /// `trace` feature this is just `f(self)`.
    #[inline(always)]
    #[allow(unused_variables)]
    fn traced<R>(&mut self, op: Op, a: u64, b: u64, f: impl FnOnce(&mut Random) -> R) -> R {
        #[cfg(feature = "trace")]
        {
            let (state, double_state) = (self.state, self.double_state);
            if let (0, Some(trace)) = (self.depth, &mut self.trace) {
                trace.push(Call {
                    op,
                    a,
                    b,
                    state,
                    double_state,
                });
            }
            self.depth += 1;
            let r = f(self);
            self.depth -= 1;
            r
        }
        #[cfg(not(feature = "trace"))]
        f(self)
    }

    /// `cRZRandom::Seed` (SIMDIRT.DLL 0x1001BB50). Seeds both generators.
    pub fn seed(&mut self, seed: u32) {
        self.traced(Op::Seed, seed as u64, 0, |r| {
            r.state = seed;
            r.double_state = seed;
        })
    }

    /// `RandomUint32Uniform()` (0x1001BB6B). The state keeps the low 32 bits of the LCG; the
    /// result is bits 16..48 of the 64-bit `state * mul + add`.
    pub fn next_u32(&mut self) -> u32 {
        self.traced(Op::NextU32, 0, 0, |r| {
            let p = r.state as u64 * LCG_MUL + LCG_ADD;
            r.state = p as u32;
            (p >> 16) as u32
        })
    }

    /// `RandomUint32Uniform(unsigned)` (0x1001BBA8). Below `0xFFFFFF` the result is
    /// `next % n`, in `0..n`. From `0xFFFFFF` up the original redraws until the value is at
    /// most `n`, so `n` itself can come out.
    pub fn uniform(&mut self, n: u32) -> u32 {
        self.traced(Op::Uniform, n as u64, 0, |r| {
            if n == 0 {
                return 0;
            }
            if n < LARGE_RANGE {
                return r.next_u32() % n;
            }
            loop {
                // The original masks the value with its own bit length first, which changes
                // nothing.
                let v = r.next_u32();
                if v <= n {
                    return v;
                }
            }
        })
    }

    /// `RandomSint32RangeUniform(lo, hi)` (0x1001BC23): `lo + uniform(hi - lo)`.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        self.traced(Op::Range, lo as u32 as u64, hi as u32 as u64, |r| {
            (r.uniform(hi.wrapping_sub(lo) as u32) as i32).wrapping_add(lo)
        })
    }

    /// `RandomSint32RangeGaussianFast(lo, hi)` (0x1001BC58). A triangle around the middle:
    /// the smaller of two draws in `0..half`, with a random sign, added to `lo + half`.
    pub fn gaussian_fast(&mut self, lo: i32, hi: i32) -> i32 {
        self.traced(Op::GaussianFast, lo as u32 as u64, hi as u32 as u64, |r| {
            let half = hi.wrapping_sub(lo) / 2;
            let a = r.uniform(half as u32) as i32;
            let b = r.uniform(half as u32) as i32;
            let mut m = if a < b { a } else { b };
            if r.uniform(2) != 0 {
                m = m.wrapping_neg();
            }
            m.wrapping_add(half).wrapping_add(lo)
        })
    }

    /// Unnamed (0x1001BCA4, thunk 0x1001BC48): `lo` plus the smaller of two draws in
    /// `0..hi - lo`, so low values are more likely.
    pub fn range_min_of_two(&mut self, lo: i32, hi: i32) -> i32 {
        self.traced(Op::RangeMinOfTwo, lo as u32 as u64, hi as u32 as u64, |r| {
            let n = hi.wrapping_sub(lo) as u32;
            let a = r.uniform(n) as i32;
            let b = r.uniform(n) as i32;
            a.min(b).wrapping_add(lo)
        })
    }

    /// `RandomDoubleUniform()` (0x1001BCD9), in `[0, 1)`. Uses its own LCG state.
    pub fn double(&mut self) -> f64 {
        self.traced(Op::Double, 0, 0, |r| {
            if r.double_state == 0 {
                r.double_state = DOUBLE_RESEED;
            }
            r.double_state = r.double_state.wrapping_mul(DOUBLE_MUL);
            r.double_state as f64 * f64::from_bits(DOUBLE_SCALE)
        })
    }

    /// `RandomDoubleRangeUniform(lo, hi)` (0x1001BD1B): `double() * (hi - lo) + lo`. The
    /// original computes this on the x87 stack in extended precision, so the last bit can
    /// differ.
    pub fn double_range(&mut self, lo: f64, hi: f64) -> f64 {
        self.traced(Op::DoubleRange, lo.to_bits(), hi.to_bits(), |r| {
            r.double() * (hi - lo) + lo
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_sequence() {
        // Hand-computed: 1 * 0x41C64E6D + 0x3039 = 0x41C67EA6.
        let mut r = Random::new(1);
        assert_eq!(r.next_u32(), 0x41C6);
        assert_eq!(r.state, 0x41C6_7EA6);
        // 0x41C67EA6 * 0x41C64E6D + 0x3039 = 0x10E6_59D4_967E_B0E7 (64-bit).
        assert_eq!(r.next_u32(), 0x59D4_967E);
        assert_eq!(r.state, 0x967E_B0E7);
    }

    #[test]
    fn uniform_ranges() {
        let mut r = Random::new(1);
        assert_eq!(r.uniform(0), 0);
        assert_eq!(r.state, 1, "a zero range draws nothing");
        assert_eq!(r.uniform(100), 0x41C6 % 100);
        let mut r = Random::new(7);
        for _ in 0..1000 {
            let v = r.range(-5, 5);
            assert!((-5..5).contains(&v));
        }
    }

    #[test]
    fn large_ranges_redraw() {
        let mut r = Random::new(1);
        // 0x41C6 is already at most 0xFFFFFF, so it comes back unchanged.
        assert_eq!(r.uniform(LARGE_RANGE), 0x41C6);
        let mut r = Random::new(1);
        // Too big for 0x1000000 the second time round, so it redraws.
        r.next_u32();
        let mut copy = r.clone();
        let v = r.uniform(0x0100_0000);
        assert!(v <= 0x0100_0000);
        assert_ne!(copy.next_u32(), v);
    }

    #[test]
    fn gaussian_stays_in_range() {
        let mut r = Random::new(12345);
        for _ in 0..10_000 {
            let v = r.gaussian_fast(10, 30);
            assert!((1..=29).contains(&v), "{v}");
        }
        // half = 0: every draw is 0, only the sign draw advances the state.
        let mut r = Random::new(3);
        assert_eq!(r.gaussian_fast(4, 5), 4);
    }

    #[cfg(feature = "trace")]
    #[test]
    fn trace_logs_outermost_calls_only() {
        let mut r = Random::new(5);
        r.next_u32();
        assert!(r.take_trace().is_empty(), "off until started");
        let mut r = Random::new(5);
        r.start_trace();
        r.gaussian_fast(-3, 9);
        let after = r.clone();
        r.double_range(1.0, 2.0);
        let t = r.take_trace();
        assert_eq!(t.len(), 2, "{t:?}");
        assert_eq!(
            (t[0].op, t[0].a, t[0].b),
            (Op::GaussianFast, (-3i32) as u32 as u64, 9)
        );
        assert_eq!((t[0].state, t[0].double_state), (5, 5));
        assert_eq!(t[1].op, Op::DoubleRange);
        assert_eq!((t[1].a, t[1].b), (1.0f64.to_bits(), 2.0f64.to_bits()));
        assert_eq!(t[1].state, after.state);
    }

    #[test]
    fn double_sequence() {
        let mut r = Random::new(0);
        let v = r.double();
        assert_eq!(r.double_state, 0x1234_5678u32.wrapping_mul(DOUBLE_MUL));
        assert!((0.0..1.0).contains(&v));
        let mut r = Random::new(2);
        let v = r.double_range(10.0, 20.0);
        assert!((10.0..20.0).contains(&v));
    }
}
