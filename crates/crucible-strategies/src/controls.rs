//! Benchmarks a result is quoted **against**, and one strategy that cheats.
//!
//! `docs/PROJECT_PLAN.md` §7.4 bans the bare dollar figure: a number is
//! reported with its denominators — % of capital, vs buy-and-hold, vs a
//! random-entry matched baseline, N trades, N independent days, trial count,
//! fill model. Two of those denominators are *runs*, not arithmetic, so they
//! live here as strategies and are replayed on the same bars, through the same
//! engine, under the same fill model as the thing they are benchmarking.
//! Anything less and the comparison is between a backtest and a formula.
//!
//! | control | question it answers |
//! |---|---|
//! | [`BuyAndHold`] | did the strategy beat *owning the thing*? |
//! | [`RandomEntry`] | did its **timing** beat coin flips that traded as often, as long, and as directionally? |
//!
//! ## Why the random control is *matched* and not just random
//!
//! An unmatched random benchmark is easy to beat and therefore worthless. The
//! three things it must not be allowed to differ in are trade count, holding
//! time, and direction mix:
//!
//! - **Count** — a strategy that trades twice cannot be compared to a control
//!   that trades two hundred times; fees alone decide it.
//! - **Holding** — exposure is time in the market, and a control that holds
//!   for a tenth as long carries a tenth of the risk.
//! - **Direction** — a strategy that was long 90 % of the time in a rising
//!   market is competing against drift, and a coin-flip control does not
//!   collect that drift. The control would lose for a reason that has nothing
//!   to do with the strategy's skill.
//!
//! So [`RandomEntry`] takes the reference run's own episodes — the multiset of
//! `(holding bars, direction)` pairs — and re-places them at seeded-random
//! times. Everything is held fixed except *when*, which is exactly the claim
//! under test.
//!
//! ## Randomness (CLAUDE.md §2.2, D-0011)
//!
//! The generator is an inlined SplitMix64 over integers, not a `rand`
//! dependency: this crate depends on `crucible-core` and nothing else, forever
//! (§3), and a seeded stream that is bit-identical across platforms and Rust
//! releases is worth more here than a better-distributed one. Every seed
//! arrives from the caller, derived from
//! `crucible_funnel::walkforward::derive_control_seed` (D-0064's lineage).
//!
//! ## And one that cheats, on purpose
//!
//! [`LeakyZScore`] is a **planted defect** (see its own docs). It is here so
//! that the permutation and truncation harnesses M3 still owes have something
//! whose detection can be watched, and so that today's honest answer — the
//! gates do *not* catch it — is written down rather than assumed.

use crucible_core::prelude::*;

/// SplitMix64, inlined. Same generator D-0011 put in the synthetic feed, for
/// the same reason: no dependency, and identical bits everywhere.
#[derive(Clone, Copy, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Seeds the stream.
    #[must_use]
    pub const fn new(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed }
    }

    /// The next 64 bits.
    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A uniform value in `0..bound`, by Lemire's multiply-shift.
    ///
    /// Not rejection-sampled: the modulo bias for a `bound` that is a bar
    /// index against a 64-bit draw is on the order of `bound / 2^64`, which is
    /// unmeasurable, and rejection would make the number of draws depend on
    /// the values drawn — turning "how many random numbers did this control
    /// consume" into something that varies with the data.
    pub const fn below(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            return 0;
        }
        let x = self.next_u64();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the product's high half is < bound by construction"
        )]
        let scaled = ((x as u128 * bound as u128) >> 64) as usize;
        scaled
    }
}

/// Own the instrument for the whole evaluation window: the first denominator
/// any equity curve has to beat.
///
/// Goes long `qty` on the bar the window opens at and never trades again, so
/// its round-trip count is 1 (or 0, if the run ends still holding — which it
/// does, because holding to the end is the point). A grid's warmup bar is the
/// right `start_bar`: the strategy under test could not act before it either,
/// and a benchmark that owned the thing for 200 bars longer is not a
/// benchmark.
#[derive(Clone, Copy, Debug)]
pub struct BuyAndHold {
    start_bar: usize,
    qty: Qty,
    bars_seen: usize,
    entered: bool,
}

impl BuyAndHold {
    /// Long `qty` from `start_bar` to the end of the series.
    ///
    /// # Panics
    /// If `qty` is zero — a benchmark that holds nothing is a config bug, and
    /// it should fail here rather than silently report a flat line as the
    /// market's return (§5.1).
    #[must_use]
    pub fn new(start_bar: usize, qty: Qty) -> BuyAndHold {
        assert!(qty.0 != 0, "a buy-and-hold control must hold something");
        BuyAndHold {
            start_bar,
            qty: Qty(qty.0.abs()),
            bars_seen: 0,
            entered: false,
        }
    }
}

impl Strategy for BuyAndHold {
    fn warmup_bars(&self) -> usize {
        self.start_bar
    }

    fn on_event(&mut self, _ev: &MarketEvent, _view: &PortfolioView, actions: &mut Actions) {
        let bar = self.bars_seen;
        self.bars_seen += 1;
        if !self.entered && bar >= self.start_bar {
            self.entered = true;
            actions.buy(self.qty);
        }
    }
}

/// One episode a [`RandomEntry`] control has to reproduce: how long the
/// reference held, and which way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchedEpisode {
    /// Bars from the entry decision to the exit decision, as the reference
    /// run's own round-trip measured them. At least 1.
    pub holding_bars: usize,
    /// [`Side::Buy`] for a long episode, [`Side::Sell`] for a short one.
    pub direction: Side,
}

/// Why a matched control could not be built. Both variants mean the control
/// is **absent**, and an absent control is reported as absent — never as a
/// zero, which reads like a benchmark that was beaten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    /// The reference run made no round-trips, so there is nothing to match.
    NothingToMatch,
    /// The episodes do not fit in the window without overlapping.
    ///
    /// Overlapping them would give the control a position size the reference
    /// never had; dropping the ones that do not fit would quietly un-match the
    /// trade count. Both are worse than saying so.
    WindowTooShort {
        /// Bars the episodes need, end to end.
        needed: usize,
        /// Bars the evaluation window has.
        available: usize,
    },
}

impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControlError::NothingToMatch => write!(
                f,
                "the reference run closed no round-trips, so a matched random-entry control has \
                 no trade count, no holding time and no direction mix to match"
            ),
            ControlError::WindowTooShort { needed, available } => write!(
                f,
                "the reference run's episodes need {needed} bars end to end but the evaluation \
                 window is {available}; placing them would have to overlap them, which gives the \
                 control a position the reference never held"
            ),
        }
    }
}

impl std::error::Error for ControlError {}

/// Trade as often, as long, and as directionally as the reference run did —
/// at seeded-random times.
///
/// The entry bars are drawn **at construction**, not during the replay, and
/// the schedule is then a fixed script. Two reasons: the draw must not depend
/// on anything the market did (a control whose randomness reacts to prices is
/// a strategy), and a `Debug`-printed schedule is a reproducible artifact a
/// reader can check against the seed.
#[derive(Clone, Debug)]
pub struct RandomEntry {
    /// `(entry bar, exit bar, direction)`, sorted by entry bar, non-overlapping.
    schedule: Vec<(usize, usize, Side)>,
    qty: Qty,
    bars_seen: usize,
    next: usize,
}

impl RandomEntry {
    /// Places `episodes` uniformly at random inside `window`, without overlap.
    ///
    /// The placement is a stars-and-bars draw: with `k` episodes consuming `h`
    /// bars in a window of `w`, there are `w - h` bars of slack to distribute
    /// into `k + 1` gaps. Drawing `k` sorted cut points in `0..=w-h` and
    /// laying the episodes end to end between them is uniform over all
    /// non-overlapping placements, and — unlike rejection sampling — consumes
    /// exactly `k` draws whatever the window looks like.
    ///
    /// # Errors
    /// [`ControlError`] if there are no episodes to match, or if they cannot
    /// fit in `window` without overlapping.
    ///
    /// # Panics
    /// If `qty` is zero, or if any episode holds for zero bars. Both are
    /// caller bugs — a round-trip that opened and closed on the same bar still
    /// spans one bar (§5.1).
    pub fn matched(
        seed: u64,
        episodes: &[MatchedEpisode],
        window: std::ops::Range<usize>,
        qty: Qty,
    ) -> Result<RandomEntry, ControlError> {
        RandomEntry::matched_across(seed, &[(window, episodes.to_vec())], qty)
    }

    /// The same, over several disjoint windows at once — the funnel's case,
    /// where each fold's test window gets the episodes that closed inside it.
    ///
    /// One RNG stream spans every window, drawn in window order, so the
    /// schedule is a function of the seed and the window list and of nothing
    /// else. Episodes are never placed across a window boundary: the gap
    /// between two test windows is a *training* window, and exposure parked
    /// there is exposure the pooled out-of-sample curve does not look at
    /// (D-0063), so a control that straddled a seam would be benchmarking with
    /// risk nobody can see.
    ///
    /// # Errors
    /// [`ControlError::NothingToMatch`] if no window has an episode, or
    /// [`ControlError::WindowTooShort`] if some window's episodes cannot fit
    /// inside it.
    ///
    /// # Panics
    /// If `qty` is zero, or if any episode holds for zero bars.
    pub fn matched_across(
        seed: u64,
        windows: &[(std::ops::Range<usize>, Vec<MatchedEpisode>)],
        qty: Qty,
    ) -> Result<RandomEntry, ControlError> {
        assert!(qty.0 != 0, "a random-entry control must trade something");
        assert!(
            windows
                .iter()
                .all(|(_, e)| e.iter().all(|e| e.holding_bars > 0)),
            "an episode spans at least one bar"
        );
        if windows.iter().all(|(_, e)| e.is_empty()) {
            return Err(ControlError::NothingToMatch);
        }

        let mut rng = SplitMix64::new(seed);
        let mut schedule = Vec::new();
        for (window, episodes) in windows {
            if episodes.is_empty() {
                continue;
            }
            let held: usize = episodes.iter().map(|e| e.holding_bars).sum();
            let available = window.len();
            if held > available {
                return Err(ControlError::WindowTooShort {
                    needed: held,
                    available,
                });
            }

            // Draw one cut point per episode, then sort. Sorting a uniform
            // sample is the standard way to get uniform *order statistics*,
            // and — unlike rejection sampling — it keeps the number of draws
            // independent of the values drawn.
            let slack = available - held + 1;
            let mut cuts: Vec<usize> = (0..episodes.len()).map(|_| rng.below(slack)).collect();
            cuts.sort_unstable();

            let mut consumed = 0usize;
            for (episode, cut) in episodes.iter().zip(cuts) {
                let entry = window.start + cut + consumed;
                schedule.push((entry, entry + episode.holding_bars, episode.direction));
                consumed += episode.holding_bars;
            }
        }

        Ok(RandomEntry {
            schedule,
            qty: Qty(qty.0.abs()),
            bars_seen: 0,
            next: 0,
        })
    }

    /// The drawn schedule: `(entry bar, exit bar, direction)`.
    #[must_use]
    pub fn schedule(&self) -> &[(usize, usize, Side)] {
        &self.schedule
    }
}

impl Strategy for RandomEntry {
    fn warmup_bars(&self) -> usize {
        self.schedule.first().map_or(0, |&(entry, _, _)| entry)
    }

    fn on_event(&mut self, _ev: &MarketEvent, view: &PortfolioView, actions: &mut Actions) {
        let bar = self.bars_seen;
        self.bars_seen += 1;
        let Some(&(entry, exit, direction)) = self.schedule.get(self.next) else {
            return;
        };
        if bar == entry {
            let target = Qty(self.qty.0 * i32::from(direction == Side::Buy) * 2 - self.qty.0);
            actions.target_position(view.position, target);
        } else if bar == exit {
            actions.target_position(view.position, Qty(0));
            self.next += 1;
        }
    }
}

/// **A planted lookahead defect.** Never use this to produce a result.
///
/// CLAUDE.md §2.1 names the failure this reproduces: *"Full-sample z-scores,
/// full-sample quantiles, or 'fit on everything then backtest' are lookahead."*
/// The strategy standardizes each close against the mean and standard
/// deviation of the **whole series**, including the part that has not happened
/// yet, and buys what is cheap relative to that. It never looks at an
/// individual future bar, which is what makes it a realistic defect rather
/// than a strawman: the code contains no obvious peek, the equity curve looks
/// ordinary, and the leak is entirely in where two `f64`s came from.
///
/// On a **random walk** it is a mint. A random walk has no mean to revert to
/// — but the full-sample mean is a summary of where the walk actually went,
/// so buying below it and selling above it is buying what is known in advance
/// to be low. That is why the null harness, which is supposed to contain no
/// edge at all, is the sharpest possible demonstration: any profit here is
/// leakage by construction.
///
/// It exists so the detectors that are still owed —
/// `crucible-funnel::stats`'s permutation nulls and the truncation-invariance
/// harness — have a defect whose detection can be *watched*, per §7's
/// no-quality-exemption clause. Until they land, the honest record is that the
/// funnel's S0–S2 gates do **not** catch it, and
/// `crucible-funnel::tests::the_planted_leak_survives_todays_gates` is the
/// test that says so out loud.
#[derive(Clone, Copy, Debug)]
pub struct LeakyZScore {
    /// Mean close of the whole series, in points. **This is the leak.**
    full_sample_mean_points: f64,
    /// Population standard deviation of the whole series, in points. Also the
    /// leak.
    full_sample_stdev_points: f64,
    /// Go long below `−entry_z`, short above `+entry_z`, and hold in between.
    ///
    /// Stop-and-reverse rather than long/flat/short: between the bands the
    /// position is *kept*, so the strategy is nearly always exposed and its
    /// direction is chosen with knowledge of where the series was going to
    /// average. That is what makes the leak's size comparable to the market's
    /// own move instead of a fraction of it — the difference between a defect
    /// a gate might miss for interesting reasons and one it misses because the
    /// defect was small.
    entry_z: f64,
    qty: Qty,
}

impl LeakyZScore {
    /// Fits on everything, then backtests. The signature is the bug: a
    /// strategy that takes the whole series before the replay starts cannot be
    /// anything else.
    ///
    /// # Panics
    /// If `closes` is empty, if `entry_z` is not finite and positive, or if
    /// `qty` is zero.
    #[must_use]
    pub fn fit_on_everything(closes: &[Price], entry_z: f64, qty: Qty) -> LeakyZScore {
        assert!(!closes.is_empty(), "nothing to fit on");
        assert!(
            entry_z.is_finite() && entry_z > 0.0,
            "entry_z must be a positive number of standard deviations"
        );
        assert!(qty.0 != 0, "a strategy must trade something");
        #[expect(clippy::cast_precision_loss, reason = "statistics space (§2.3)")]
        let n = closes.len() as f64;
        let mean = closes.iter().map(|p| p.as_points_f64()).sum::<f64>() / n;
        let var = closes
            .iter()
            .map(|p| {
                let d = p.as_points_f64() - mean;
                d * d
            })
            .sum::<f64>()
            / n;
        LeakyZScore {
            full_sample_mean_points: mean,
            full_sample_stdev_points: var.sqrt(),
            entry_z,
            qty: Qty(qty.0.abs()),
        }
    }
}

impl Strategy for LeakyZScore {
    fn warmup_bars(&self) -> usize {
        // None, and that is itself the tell: an honest z-score has a window to
        // warm up, and this one already knows the answer.
        0
    }

    fn on_event(&mut self, ev: &MarketEvent, view: &PortfolioView, actions: &mut Actions) {
        if self.full_sample_stdev_points <= 0.0 {
            return;
        }
        let MarketEvent::Bar(bar) = ev;
        let z = (bar.close.as_points_f64() - self.full_sample_mean_points)
            / self.full_sample_stdev_points;
        let target = if z <= -self.entry_z {
            self.qty
        } else if z >= self.entry_z {
            Qty(-self.qty.0)
        } else {
            view.position
        };
        actions.target_position(view.position, target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(i: i64, close: f64) -> MarketEvent {
        let p = Price::from_points_f64_lossy(close);
        MarketEvent::Bar(Bar {
            instrument: InstrumentId::new("SYN:TEST"),
            tf: TimeFrame::M1,
            ts_open: Ts(i * 60_000_000_000),
            open: p,
            high: p,
            low: p,
            close: p,
            volume: 1,
        })
    }

    /// Drives a strategy over `n` flat bars and returns the bar index of every
    /// intent, with the signed quantity it asked for.
    fn intents<S: Strategy>(strategy: &mut S, n: usize) -> Vec<(usize, i32)> {
        let mut position = Qty(0);
        let mut out = Vec::new();
        for i in 0..n {
            let view = PortfolioView {
                position,
                avg_entry: None,
                cash_nano_usd: 0,
                equity_nano_usd: 0,
            };
            let mut actions = Actions::new();
            #[expect(clippy::cast_possible_wrap, reason = "test index")]
            strategy.on_event(&bar(i as i64, 100.0), &view, &mut actions);
            for intent in actions.take_intents() {
                let delta = intent.qty.0 * i32::try_from(intent.side.sign()).expect("±1");
                out.push((i, delta));
                position = Qty(position.0 + delta);
            }
        }
        out
    }

    /// Buy-and-hold enters once, on the bar the window opens at, and then does
    /// nothing at all.
    #[test]
    fn buy_and_hold_enters_on_the_first_evaluable_bar_and_never_again() {
        let mut c = BuyAndHold::new(5, Qty(2));
        assert_eq!(intents(&mut c, 40), vec![(5, 2)]);
    }

    fn episodes(spec: &[(usize, Side)]) -> Vec<MatchedEpisode> {
        spec.iter()
            .map(|&(holding_bars, direction)| MatchedEpisode {
                holding_bars,
                direction,
            })
            .collect()
    }

    /// The matching claim, asserted rather than described: same number of
    /// round-trips, same holding lengths, same direction mix.
    #[test]
    fn a_matched_control_trades_as_often_as_long_and_as_directionally() {
        let want = episodes(&[(3, Side::Buy), (7, Side::Sell), (2, Side::Buy)]);
        let c = RandomEntry::matched(0xfeed_1234, &want, 10..200, Qty(1)).expect("fits");

        assert_eq!(c.schedule().len(), want.len());
        let holdings: Vec<usize> = c.schedule().iter().map(|&(a, b, _)| b - a).collect();
        assert_eq!(holdings, vec![3, 7, 2]);
        let sides: Vec<Side> = c.schedule().iter().map(|&(_, _, s)| s).collect();
        assert_eq!(sides, vec![Side::Buy, Side::Sell, Side::Buy]);
    }

    /// Placements never overlap and never leave the window — the property that
    /// keeps the control's position size equal to the reference's.
    #[test]
    fn matched_episodes_never_overlap_and_stay_inside_the_window() {
        let want = episodes(&[(5, Side::Buy); 8]);
        for seed in 0..64u64 {
            let c = RandomEntry::matched(seed, &want, 100..300, Qty(1)).expect("fits");
            let mut prev_exit = 100;
            for &(entry, exit, _) in c.schedule() {
                assert!(
                    entry >= prev_exit,
                    "seed {seed}: {entry} before {prev_exit}"
                );
                assert!(exit <= 300, "seed {seed}: {exit} past the window");
                prev_exit = exit;
            }
        }
    }

    /// Same seed, same schedule; different seed, different schedule. Both
    /// halves matter: the first is §2.2, the second is that the control is
    /// actually random.
    #[test]
    fn the_schedule_is_seeded_and_the_seed_moves_it() {
        let want = episodes(&[(4, Side::Buy), (4, Side::Sell)]);
        let a = RandomEntry::matched(7, &want, 0..500, Qty(1)).expect("fits");
        let b = RandomEntry::matched(7, &want, 0..500, Qty(1)).expect("fits");
        let c = RandomEntry::matched(8, &want, 0..500, Qty(1)).expect("fits");
        assert_eq!(a.schedule(), b.schedule());
        assert_ne!(a.schedule(), c.schedule());
    }

    /// An absent control is an error, never a flat equity curve that reads as
    /// a benchmark of zero.
    #[test]
    fn a_control_that_cannot_be_matched_refuses() {
        assert_eq!(
            RandomEntry::matched(1, &[], 0..100, Qty(1)).err(),
            Some(ControlError::NothingToMatch)
        );
        let want = episodes(&[(60, Side::Buy), (60, Side::Sell)]);
        assert_eq!(
            RandomEntry::matched(1, &want, 0..100, Qty(1)).err(),
            Some(ControlError::WindowTooShort {
                needed: 120,
                available: 100
            })
        );
    }

    /// Across several windows, every episode stays inside the window it was
    /// matched from — never parked in the training gap between two of them,
    /// where the pooled out-of-sample curve cannot see it.
    #[test]
    fn episodes_never_straddle_the_gap_between_two_windows() {
        let windows = vec![
            (100..140, episodes(&[(5, Side::Buy), (5, Side::Sell)])),
            (300..340, episodes(&[(9, Side::Buy)])),
        ];
        for seed in 0..32u64 {
            let c = RandomEntry::matched_across(seed, &windows, Qty(1)).expect("fits");
            assert_eq!(c.schedule().len(), 3);
            for &(entry, exit, _) in c.schedule() {
                let inside = ((100..=140).contains(&entry) && (100..=140).contains(&exit))
                    || ((300..=340).contains(&entry) && (300..=340).contains(&exit));
                assert!(inside, "seed {seed}: {entry}..{exit} straddles a window");
            }
        }
    }

    /// The scheduled entries and exits actually reach the order stream, in
    /// order, with the drawn directions.
    #[test]
    fn the_schedule_is_what_the_control_trades() {
        let want = episodes(&[(2, Side::Sell), (3, Side::Buy)]);
        let mut c = RandomEntry::matched(99, &want, 0..40, Qty(1)).expect("fits");
        let schedule: Vec<(usize, usize, Side)> = c.schedule().to_vec();
        let traded = intents(&mut c, 40);

        let mut expected = Vec::new();
        for &(entry, exit, direction) in &schedule {
            let signed = i32::try_from(direction.sign()).expect("±1");
            expected.push((entry, signed));
            expected.push((exit, -signed));
        }
        assert_eq!(traded, expected);
    }

    /// The leak, demonstrated on data with no edge in it: a deterministic
    /// zig-zag whose second half sits above its first. An honest strategy
    /// cannot know that at bar 0. This one is *built* from it, so it is short
    /// the low half and long the high half — backwards, profitably, and by
    /// construction.
    #[test]
    fn the_planted_leak_trades_on_a_statistic_from_the_future() {
        let closes: Vec<Price> = (0..100)
            .map(|i| Price::from_points_f64_lossy(if i < 50 { 90.0 } else { 110.0 }))
            .collect();
        let leaky = LeakyZScore::fit_on_everything(&closes, 0.5, Qty(1));
        // Mean 100, stdev 10 ⇒ the first bar's z is −1, beyond the 0.5 entry.
        assert!((leaky.full_sample_mean_points - 100.0).abs() < 1e-9);
        assert!((leaky.full_sample_stdev_points - 10.0).abs() < 1e-9);

        // And it acts on bar 0, having consumed no history whatsoever.
        let view = PortfolioView {
            position: Qty(0),
            avg_entry: None,
            cash_nano_usd: 0,
            equity_nano_usd: 0,
        };
        let mut actions = Actions::new();
        let mut leaky = leaky;
        leaky.on_event(&bar(0, 90.0), &view, &mut actions);
        let first = actions.take_intents();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].side, Side::Buy);
        assert_eq!(leaky.warmup_bars(), 0);
    }

    /// The generator is pinned: a control seeded with a stored seed must draw
    /// the same schedule in five years' time.
    ///
    /// The first two values are Vigna's **published** SplitMix64 outputs for
    /// seed 0, so this is a check against an external reference and not
    /// against our own last run. The third is then derived from the second
    /// reference stream: seed 42's first draw is `0xbdd7_3226_2feb_6e95`, and
    /// `below(1000)` is `(0xbdd7_3226_2feb_6e95 × 1000) >> 64` = 741.
    #[test]
    fn the_generator_is_pinned() {
        let mut rng = SplitMix64::new(0);
        assert_eq!(rng.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(rng.next_u64(), 0x6e78_9e6a_a1b9_65f4);
        assert_eq!(SplitMix64::new(42).next_u64(), 0xbdd7_3226_2feb_6e95);
        assert_eq!(SplitMix64::new(42).below(1000), 741);
    }
}
