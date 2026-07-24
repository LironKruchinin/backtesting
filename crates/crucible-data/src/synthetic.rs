//! Deterministic synthetic market data.
//!
//! Purposes: (1) engine tests and the demo; (2) the null-hypothesis harness —
//! a strategy that "finds edge" on a seeded random walk is exhibiting either
//! a lookahead bug or overfitting, and the funnel's permutation stage (M3)
//! is built on exactly this property.
//!
//! Determinism rules: the generator is pure integer arithmetic on ticks with
//! an inlined SplitMix64 — no floats in price construction, no platform-
//! dependent behavior, no `rand` dependency. Same seed ⇒ bit-identical bars
//! on every machine, forever. The base timestamp is a fixed constant, not
//! wall-clock (CLAUDE.md §2.2).

use crucible_core::prelude::*;

/// SplitMix64 PRNG — tiny, fast, deterministic. Fine for synthetic data;
/// NOT for anything cryptographic.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform draw in `0..=bound` (modulo bias is irrelevant here).
    fn next_bounded(&mut self, bound: u64) -> u64 {
        self.next_u64() % (bound + 1)
    }
}

/// Fixed base timestamp: 2023-11-14T22:13:20Z. A constant, deliberately —
/// synthetic feeds must not depend on when they were generated.
const BASE_TS_NS: i64 = 1_700_000_000_000_000_000;

/// Price floor in ticks so a long random walk cannot go nonpositive.
const MIN_PRICE_TICKS: i64 = 400;

/// A seeded random-walk bar feed. All prices are exact multiples of `tick`.
#[derive(Debug, Clone)]
pub struct SyntheticFeed {
    rng: SplitMix64,
    remaining: usize,
    instrument: InstrumentId,
    tf: TimeFrame,
    tick_nanos: i64,
    price_ticks: i64,
    vol_ticks: u64,
    next_ts_open: Ts,
}

impl SyntheticFeed {
    /// # Panics
    /// Panics if `tick` is nonpositive or `start` is not tick-aligned
    /// (config bug — fail at construction).
    #[must_use]
    pub fn random_walk(
        seed: u64,
        n_bars: usize,
        tf: TimeFrame,
        start: Price,
        tick: Price,
        vol_ticks: u64,
    ) -> Self {
        assert!(tick.as_nanos() > 0, "tick must be positive");
        assert!(
            start.as_nanos() % tick.as_nanos() == 0,
            "start price must be tick-aligned"
        );
        SyntheticFeed {
            rng: SplitMix64::new(seed),
            remaining: n_bars,
            instrument: InstrumentId::new("SYN:RW"),
            tf,
            tick_nanos: tick.as_nanos(),
            price_ticks: start.as_nanos() / tick.as_nanos(),
            vol_ticks: vol_ticks.max(1),
            next_ts_open: Ts(BASE_TS_NS),
        }
    }

    fn price(&self, ticks: i64) -> Price {
        Price::from_nanos(ticks * self.tick_nanos)
    }
}

impl Feed for SyntheticFeed {
    fn next_event(&mut self) -> Option<MarketEvent> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;

        // Fixed draw count per bar keeps the stream alignment stable.
        let span = 2 * self.vol_ticks;
        #[expect(clippy::cast_possible_wrap, reason = "vol_ticks is tiny")]
        let delta = self.rng.next_bounded(span) as i64 - self.vol_ticks as i64;
        #[expect(clippy::cast_possible_wrap, reason = "vol_ticks is tiny")]
        let wick_up = self.rng.next_bounded(self.vol_ticks) as i64;
        #[expect(clippy::cast_possible_wrap, reason = "vol_ticks is tiny")]
        let wick_dn = self.rng.next_bounded(self.vol_ticks) as i64;
        let volume = 100 + self.rng.next_bounded(900);

        let open_ticks = self.price_ticks;
        let close_ticks = (open_ticks + delta).max(MIN_PRICE_TICKS);
        let high_ticks = open_ticks.max(close_ticks) + wick_up;
        let low_ticks = (open_ticks.min(close_ticks) - wick_dn).max(1);

        let bar = Bar {
            instrument: self.instrument.clone(),
            tf: self.tf,
            ts_open: self.next_ts_open,
            open: self.price(open_ticks),
            high: self.price(high_ticks),
            low: self.price(low_ticks),
            close: self.price(close_ticks),
            volume,
        };

        self.next_ts_open = self.next_ts_open.plus_ns(self.tf.duration_ns());
        self.price_ticks = close_ticks;
        Some(MarketEvent::Bar(bar))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed() -> SyntheticFeed {
        SyntheticFeed::random_walk(
            42,
            500,
            TimeFrame::M1,
            Price::from_points(5000),
            Price::from_points_f64_lossy(0.25),
            4,
        )
    }

    #[test]
    fn same_seed_same_bars() {
        let (mut a, mut b) = (feed(), feed());
        for _ in 0..500 {
            assert_eq!(a.next_event(), b.next_event());
        }
        assert_eq!(a.next_event(), None);
    }

    #[test]
    fn bars_are_ordered_and_ohlc_sane() {
        let mut f = feed();
        let mut prev_avail: Option<Ts> = None;
        while let Some(MarketEvent::Bar(b)) = f.next_event() {
            assert!(b.high >= b.open && b.high >= b.close);
            assert!(b.low <= b.open && b.low <= b.close);
            assert!(b.low.as_nanos() > 0, "prices stay positive");
            let avail = b.avail_ts();
            if let Some(p) = prev_avail {
                assert!(avail > p, "strictly increasing avail_ts");
            }
            prev_avail = Some(avail);
        }
    }
}
