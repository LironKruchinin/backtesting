# Milestones

Scope discipline: each milestone ends with something demoable and honest.
If the semester ends mid-M3, the project is still a complete, defensible
artifact. Effort estimates assume part-time (student) pace.

## M0 — Skeleton ✅ (2026-07-24)

Workspace, core types with the availability-time invariant, deterministic
single-threaded engine with named fill models, integer-only accounting,
SMA/EMA/Bollinger indicators, reference SMA-cross strategy, seeded synthetic
feeds, golden + determinism tests, CI with a determinism gate, `demo`
command, decision log, this plan.

## M1 — Data foundation (~2–3 weeks)

The archive is the asset; rolling entitlement windows make archiving
deadline-driven (see `crucible-data::ingest` module docs).

- [x] Archive catalog (`crucible-data::catalog`, 2026-07-24): append-only
      `manifest.jsonl`, blake3 checksums as manifest ids, per-symbol coverage
      (requested minus owned), integrity `verify`, hard-error load validation
- [x] `crucible pull` (2026-07-26): Databento batch download → `raw/`
      (`.dbn.zst`), each slice recorded through `Catalog::append`;
      `Catalog::coverage` decides what is actually requested, so nothing is
      paid for twice
      - [x] Quote path (`ingest::{plan,quote}`, 2026-07-24): coverage-subtracted
            month-aligned planning, live dataset-range clipping, per-window
            `get_cost`/`get_billable_size`, exact nano-USD spending gate,
            metered-vs-billed entitlement check. Spends nothing; no caller for
            `BatchProvider::submit` yet
      - [x] Execute path (`ingest::{execute,journal,databento}`, 2026-07-26):
            submit → poll → download → verify → append, with a crash-resumable
            job journal outside `raw/`, a single-instance pull lock, and
            vendor-side reconciliation so a lost journal cannot buy twice
            (D-0028..D-0035)
      - [x] `ManifestRecord.symbols` = requested key ∪ raw symbols observed in
            the delivered DBN metadata (the assumption the validation slice
            exists to prove). Held for five of seven parents only until
            2026-07-30: the symbol predicate banned whitespace for every symbol
            rather than only for the requested key, so CME's spaced spread names
            were dropped and 21,736 of 108,696 observed symbols never reached
            the manifest (D-0066). Predicate narrowed, and the eight affected
            records credited by appended supplement lines — `sym_audit` reads 0
            missing, 0 dropped archive-wide (D-0068)
      - [x] CLI wiring: `clap`, `--execute`, `--max-cost-usd`, exit codes
            (0 done / 2 usage / 3 refused / 4 failed / 5 resumable)
- [x] Bootstrap pulls (2026-07-30): 16y `ohlcv-1s`/`ohlcv-1m` + `definition` +
      `statistics` for the seven-parent basket in `docs/DATA_PLAN.md` — the
      acquisition itself, run against a live Standard subscription per
      `docs/RUNBOOK_BLITZ.md` and `docs/BLITZ_CHECKLIST.md`. Tooling was done;
      this box was the shopping trip. **33 intents, every plan item appended**,
      each carrying `intended → submitted → downloaded → appended` in
      `jobs.jsonl`: `ohlcv-1m` 9 (ES split ×3), `ohlcv-1s` 7, `definition` 7,
      `statistics` 7, and one each of `mbo`/`tbbo`/`trades` on ES. 21.30 GiB
      across 33 raw files; `verify` re-hashed all 33 clean and `layout-check`
      reported the tree clean, both exit 0 on 2026-07-30. The seven `statistics`
      parents were the last in: quoted **$0.0000** under a flat-rate
      entitlement, six of them adopted on re-run under their original
      `GLBX-20260729-*` job ids with `submitted 0 job(s)`, so nothing was bought
      twice (D-0029/D-0034). The per-contract half of the record needed the
      D-0068 repair before it told the whole truth
      - [x] `definition`, 16y, ES/NQ/RTY (2026-07-28): pulled ahead of the
            subscription because the roll table needs expiries. Quoted
            **$0.0000** — a flat-rate entitlement was already active on the
            account, so the ~$0.15 metered estimate was never charged.
            12.06 MB across three files, `verify` clean. The first attempt
            died on an HTTP 504 mid-poll (exit 4) and re-running the identical
            command adopted all three jobs and submitted nothing twice — the
            resume path working exactly as D-0029/D-0034 designed it
- [ ] Monthly archival job for the rolling L1/L3 windows — `trades`, `tbbo`,
      `mbo`; `mbp-10` deliberately excluded (D-0023). Documented cron in
      `docs/RUNBOOK_BLITZ.md`, later automated; runs `--max-cost-usd 0.00` so
      it refuses rather than bills if the entitlement lapses
- [x] `crucible transcode` (2026-07-28): DBN → curated Parquet, one file per
      (instrument, timeframe, source window), integer columns end to end,
      schema + transcoder versions in the file's own metadata (D-0036, D-0037)
- [x] `ParquetBarFeed` implementing `Feed` (2026-07-28): availability-ordered,
      every failure resolved at `open` because `Feed` has no error channel.
      Loaded into RAM rather than mmap'd — Parquet pages are encoded and
      compressed, so there is nothing to map; the spec said "mmap'd" before
      the format was chosen
- [x] `crucible backtest` (2026-07-28): the exit artifact — SmaCross on
      archived bars under `spread_cross`, printing its own assumptions
      (D-0038)
- [x] Session calendar v1 (2026-07-28): Globex sessions, holiday/early-close
      rules with sources cited, `bars_per_year` — a compiled-in TOML table,
      `chrono`/`chrono-tz` confined here, and `backtest` annualizing from it
      instead of from the sample (D-0039)
      - [x] Validated per era against 16 years of bars, and extended to the four
            products that had none (2026-07-30, D-0086). A calendar now carries
            session **eras**: equity index gains era 2 (close 16:15) and era 3a
            (close 16:00 **with** the 15:15–15:30 CT halt, which D-0040 had
            deleted from a January-2024 sample and which the archive shows real
            on 2,018 dates and gone from 2021-06-28 exactly). `valid_from` moves
            2015-09-21 → 2012-11-19, so unmodelled history drops 5.3 y → 2.4 y;
            era 1 stays documented rather than modelled and `docs/SESSION_ERAS.md`
            §1.1 says why. `cme_globex_energy`, `cme_globex_metals`,
            `cme_globex_fx` and `cme_globex_rates` join the bundle, derived from
            the archive and cross-checked against CME — MLK 2022 alone gives four
            different closes across them. D-0059's deferred `weekday_before`
            defect is closed. **Two genuine archive holes found and reported, not
            re-pulled**: GC 2012-09-12 and ZN 2014-10-03, one session each
- [x] Continuous contracts v1 (2026-07-28): versioned volume-crossover roll
      table under `curated/rolls/`, back-adjustment applied at load and never
      stored, and `AdjustedPrice` as a distinct type so a back-adjusted level
      cannot reach `pnl_nano_usd` (D-0041..D-0046)
      - [x] Wired into the replay path (2026-07-30): `backtest --instrument
            ES.v.0 / ES.c.0` (D-0076). `ContinuousFeed` had been complete and
            unused; a `Bar` now carries `signal_offset` beside its tradeable
            prices, so indicators read `bar.signal_*()` while fills, marks and
            `pnl_nano_usd` read the then-front contract's real prices. The
            offset is zero everywhere else, so no golden value and no
            determinism hash moved. 16 years of ES: **5,640,031 bars, 66
            contracts, 65 rolls, 129,536 round trips, −$3,343,328.75 under
            `spread_cross`** — the fill model, not the signal. `combo` and
            `walk-forward` stay outright-only, because a rule comparing a price
            to an absolute constant is not safe on a back-adjusted series.
            **Still owed to M2:** a roll is a position event; a position carried
            through one pays no spread and books the raw gap, bounded at $56,950
            on that run and printed with it
      - [x] Expiries carry an availability (2026-07-31): D-0090 supersedes
            D-0046's expiry refusal with
            `expiry(contract, decision_ts) = max_by(ts_recv <= decision_ts)`.
            The vendor restates 4 contracts of 1,002 and the later record
            always carries the *earlier* expiry, so `max(expiration)` would
            pick the stale one every time. GC, ZN and 6E had exited 4 on both
            rules; **all seven roots now build `.v` and `.c`** (14 tables
            written). `.v` rows unchanged everywhere; `.c` moves ZN by 2 rows
            and 6E by 1, GC by none. All three determinism hashes unmoved
- [x] Data QA report (2026-07-28): coverage against the calendar, gaps,
      out-of-session bars, zero-volume runs, robust spike detection, DST
      boundaries, and the vendor's own `condition.json` — `crucible qa`,
      exit 4 on findings (D-0040). Its first real run corrected the calendar
      - [x] Archive layout enforced (2026-07-28): `docs/DATA_LAYOUT.md` pins
            the tree and `crucible layout-check` refuses on any departure —
            eight violation classes, each with a negative control, exit 4
            (D-0049, D-0072). Complements `verify`: shape, not bytes

**Acceptance:** one command takes a fresh machine (with API key) to a
validated local ES archive; the demo strategy runs on real ES 1m bars.
*Second half met 2026-07-28:* `pull → verify → transcode → backtest` on real
ESH4 January-2024 1m bars, 30,167 bars, −23.51% under `spread_cross`.
*Reproduced bit-identically 2026-07-30* after D-0072 re-keyed the curated store;
the contract is now spelled `ESH2024` and `--instrument ESH4` still resolves to
it while it is the only ESH the store holds. *Reproduced again the same day*
after D-0076 gave every `Bar` a signal channel — same 30,167 bars, −23.51%, 665
round trips, 27.1% win rate, $76,486.25 — and the same command now also runs on
the whole stitched sixteen years as `--instrument ES.v.0`.

## M2 — Engine hardening + combos (~3–4 weeks)

- [x] Warmup alignment (2026-07-29): `Grid::max_warmup_bars` is the max across
      the grid and `strategies::align::Aligned` enforces it, so every combo
      places its first order on the same bar index and a short-warmup combo
      gains nothing (CLAUDE.md §2.6). *Not* funnel-controlled as this line
      originally said — it is a `Strategy` decorator, so hand-written
      strategies get it too and the engine loop keeps its single job
      (D-0061). The proof is `a_short_warmup_combo_gains_nothing_from_being_short`,
      which asserts the head start exists unaligned and is gone aligned
- [x] Walk-forward runner (2026-07-29): anchored + rolling folds over one
      shared bar series, in `crucible-funnel::walkforward`, driven by
      `crucible walk-forward`. Fold boundaries are **trading days**, not
      wall-clock months — a "6-month" window holds 122–130 CME sessions
      depending on where it lands, so a month-denominated layout has a sample
      size the exchange's holiday schedule picks (D-0062). A fold is a metric
      *window* over one replay rather than a separate backtest, with an anchor
      bar, rebasing to declared capital, and delta-stitched pooling (D-0063);
      that is what takes over slicing the eval window out of the metrics, so
      D-0061's warmup-prefix caveat retires for this output and remains for
      `combo`. Per-fold seeds derive from
      `(config_hash, [run].seed, combo_index, fold)` (D-0064). The proof is
      `walkforward::tests`: a 72-bar fixture whose fold boundaries and per-fold
      statistics — including one Sharpe in closed form, −√21 — are hand-derived
      in comments, and whose training-window trade is exactly the difference
      between the whole-run and out-of-sample headline
- [x] Config-driven combo strategies (2026-07-29): tagged indicator slots with
      parameter axes, a rule grammar parsed to an AST at load time, mixed-radix
      grid expansion, and a factory — all plain data in
      `crucible-strategies::combo`, with `serde`/`toml` in `crucible-cli::config`
      and blake3 config identity computed by the caller (D-0060). `crucible
      combo` expands a config and replays every point on one shared bar series.
      `SmaCross` written as four lines of TOML emits an identical order stream,
      which is the test that makes the layer credible
- [x] Stops/targets with worst-case intrabar ordering; flag path-sensitive
      results in outputs (2026-07-30): a `Bracket` rides along with the order
      that opens a position and resolves against the price that order *actually
      filled at*, so the position is protected from the bar it was opened on
      rather than the one after. One named convention decides what an OHLC bar
      refuses to say — `stop_first_intrabar` in `crucible-engine::bracket`
      (D-0069): the opening print wins where it settles the ordering (a gap
      fills **at the open**, never at the level, in both directions), and
      otherwise a bar touching both levels fills the **stop**. `FillModel`
      gained a required `fill_protective_exit`, because a stop crosses the
      spread and a target does not, and a defaulted method would be an
      execution assumption nobody named (§2.4). Bars where the convention chose
      the outcome are counted in `BacktestResult::path_sensitive_bars` and
      printed beside the fill model by `backtest`, `combo` and `walk-forward`.
      The proof is `bracket_golden.rs`: hand-derived fixtures for both the
      ambiguous bar and the gap, each stating the value a different convention
      would have produced, plus the negative controls (one-legged, unambiguous,
      and unbracketed runs all report zero). Not a config axis yet — the combo
      grammar cannot declare a bracket, and both grid commands say so rather
      than printing a bare zero
- [x] Account-evaluation series capture (2026-07-30): the engine half of
      `docs/ACCOUNT_EVAL_SPEC.md` §3 (D-0067, D-0071) — `crucible-engine::series`
      plus `opened_ts` and MAE/MFE on `ClosedTrade`. Four series: per-trading-day
      PnL, the intraday unrealized-equity high-water, per-round-trip excursions,
      and the worst-day pair derived from the days. Captured *inside*
      `replay.rs` step 2's mark loop and never rebuilt from OHLC afterwards,
      because a rebuild re-opens the intrabar ordering `stop_first_intrabar`
      already settled (D-0069) and measures a path the account never took: the
      §5.12 control puts a number on it — a $150 peak that never existed plus
      the whole $100 drawdown, out of one bar — while a single-tick fixture and
      a rebuild adopting the engine's own convention both agree exactly, which
      is what makes the divergence attributable to the convention and nothing
      else. The trading day arrives as caller-supplied `&[i64]` keys, the same
      slice `FoldPlan::build` takes, so fold attribution and day slicing
      reconcile to the nanodollar and a planted $100 daily-loss breach names day
      7 in both consumers. Retained artifact is 56 bytes a session — 226 KB for
      16 years, against 4.98 GiB for a per-bar series — and both sizes are
      pinned by test. Every control was mutation-verified: 30 planted defects,
      each watched failing the control it targets, and the one control that did
      **not** fire (`a_flip_resets_the_excursions_with_the_episode`, blind to a
      missing excursion reset) was joined by a fixture that does. **Capture
      only:** breach probability, the block bootstrap, P(pass) and payout
      cadence are spec §4 and land with the funnel in M3
- [ ] Golden tests vs an external reference (NautilusTrader or hand-audited
      runs) on identical data — the "why should anyone trust your engine"
      answer
- [ ] Indicator numerics: replace rolling sums with rebased/Welford updates
- [ ] Criterion benches: bars/sec single-run baseline on the 5800X3D

**Acceptance:** a combo defined purely in TOML backtests on real ES data
with reproducible, externally-cross-checked results.

## M3 — Funnel + statistics (~3–4 weeks)

The quant-research payload. Specs live in `crucible-funnel` module docs.

- [x] Grid expansion + blake3 config identity; combo-count guardrails
      (2026-07-30). *Expansion and identity landed early with M2's combo
      layer* (D-0060): `ComboSpec::canonical_form` + blake3 in the caller,
      `ComboId` = (config hash, combo index). The funnel's half is here:
      `grid::check_size` **refuses** above 50,000 combos where `combo` merely
      warns above 10,000, because a funnel run charges a trial per combo and
      that damage outlives the run — and dedupe on
      `(config_hash, account_id, combo_index, fold, seed)`
- [x] Registry: runs, metrics, hypotheses, trials, verdicts;
      insert-before-run; dedup/resume (2026-07-30). **Append-only JSONL, not
      DuckDB** — `duckdb 1.10505.0 --features bundled` fails to build on this
      toolchain (missing vendored header, MSVC `C1083`), so the backend
      changed and the five contract rules did not (D-0074). A trial is
      (config, account, combo); folds of one combo are one trial and a second
      account is a new one (D-0067). An unknown line kind is refused, never
      skipped
- [x] Rayon scheduler: run-level parallelism (2026-07-30) — combos across the
      pool, merged by grid index by construction rather than by a sort, with
      `the_parallel_scheduler_agrees_with_the_serial_one` asserting the two
      reports are bit-identical including the captured account series. The
      **dataset semaphore and the multi-instance pass are deferred**: one
      config runs one instrument today so there is exactly one resident
      dataset to bound, and the multi-instance pass is a throughput change,
      which §7 wants criterion evidence for
- [x] Stages **S1–S2** with pre-registered kill criteria from config
      (2026-07-30). Criteria live in `[funnel]`, are read before the run and
      stored verbatim on the registry row inserted before the run. **S0 and S3
      are refused, not skipped** (D-0075) — S0 needs a continuous score the
      combo grammar does not produce, S3 needs `stats` — and because S3 is
      what "survived the full battery" means, this build **cannot award
      `Graduate`**; the ceiling is `Iterate` and every report says so
- [x] The two mandatory controls (2026-07-30): buy-and-hold, and a matched
      random-entry benchmark reproducing each combo's trade count, holding
      lengths and long/short mix at seeded-random times — the median of 16
      draws, with the count of draws beaten reported beside it. An absent
      control fails its criterion rather than rendering as a zero
- [x] Automated cost sweep on every combo (2026-07-30): 0 / 0.5 / 1 / 2 ticks,
      each a separate replay rather than an adjustment to a finished curve.
      Half a tick needed the fill model to carry a *distance* rather than a
      tick count (D-0073), which left every existing hash byte-identical
- [x] Account-evaluation capture wired into the run path (2026-07-30): the CLI
      computes the trading-day keys once and hands the same slice to
      `FoldPlan::build` and to the engine's `AccountCapture`, so fold
      attribution and day slicing cannot disagree (D-0071). Per-combo
      out-of-sample worst-day close/trough pairs join the scorecard. **The
      bootstrap evaluator — breach probability, P(pass), payout cadence — is
      `ACCOUNT_EVAL_SPEC.md` §4 and is the next block *of that spec***; M3's
      first block is the S0 predictor seam below (D-0081), and the two share a
      block bootstrap rather than competing for the slot
- [x] HTML scorecards with the mandatory honesty box (2026-07-30):
      self-contained, no JavaScript and no network, and `render` **refuses to
      produce a file at all** when any honesty-box field is empty. The three
      sections this build cannot compute are rendered as named holes rather
      than omitted
- [ ] **The S0 predictor seam — M3's first block** (D-0081). A score-emitting
      evaluation path with forward-return joins: a signal emits a continuous
      score per bar, the seam joins it to the return over configured horizons
      ahead, buckets, and reports — no orders, no fills, no equity curve. This
      is `docs/PROJECT_PLAN.md` §6's **M2.5 predictor workbench arriving as the
      funnel's S0** rather than beside it, because a report produced outside the
      funnel charges no trial and reaches no registry. Two halves, both owed:
      `stages`' module doc already specifies quantile buckets, monotonicity and
      a nonzero IC; `research/backlog/H-008-short-horizon-overreaction.md`
      specifies horizons at 1/5/10/20 minutes, a block bootstrap over sessions,
      and the effect size **in ticks** so it can be compared against the spread
      — which is the half that answers "real, and smaller than our costs". It
      is first because **S0-refused stops the front of the funnel** for the six
      backlog files carrying a predictor-first Gate 0 (H-001, H-008, H-011,
      H-012, H-013, H-014): the registered gate order is binding, so none of
      them can advance by running an equity-curve gate first. The `s0` load
      refusal lifts in the commit where S0 runs, never earlier (D-0075). The
      negative control is a "signal" that IS the forward return, watched firing
      on a deliberately reversed join — a forward return is §2.1 lookahead the
      instant it can reach a decision, and this build's whole job is to keep it
      in measurement space
      - [x] **The measurement half** (`crucible-funnel::s0`, 2026-07-30,
            D-0082): the forward-return join, the information coefficient, the
            equal-count quantile buckets and the session block bootstrap.
            Horizons are **durations, not bar counts**, and a window running off
            the end of the series is unanswerable rather than short-windowed —
            the second was found by a failing test, not designed in. **The
            negative control was planted before any real use and watched
            firing:** the same signal-that-is-the-forward-return scores
            IC 1.000000 through the leaky join and −0.026527 through the correct
            one, and collapses to 0.277185 at a mismatched horizon, which is the
            third case naming the cause. Three mutations each watched failing
            and restored; the one that matters is correlating the score against
            itself, caught by the *silent* control. `rand_chacha` adopted, its §6
            placeholder deleted. All four determinism hashes unmoved, because
            nothing calls it yet
      - [x] **The caller** (2026-07-31, D-0085): `ComboScorer` emits one score
            per bar from the same spec the strategy is built from; `[s0]`
            declares the score slot, horizons in minutes, buckets, draws and
            `min_abs_ic`, required exactly when `stages` names `s0`; every combo
            gets a registry row claimed before it is measured and a trial
            charged. S0 runs **ahead of** S1/S2 and its reading reaches
            `assess`, so a score that predicts nothing dies before any equity
            curve is judged. **The null harness forced the criterion**: `|IC|`
            alone measured 0.0378 on a random walk and passed a 0.02 bar, so the
            rule became `|IC| >= min` AND a bootstrap interval excluding zero,
            **at the same horizon** — the criterion was corrected rather than
            the threshold fitted. `crucible funnel --config configs/s0-smoke.toml`
            now exits 5 with 6/6 **KILL at s0**. S0 hash **`91107aeb6e6802c0`**;
            the three engine hashes and the old funnel hash are unmoved.
            `Stage::S0::is_implemented()` is true, `s0` configs are accepted and
            H-008 is unblocked — all in that commit, as D-0075 asked
- [ ] Stats: deflated Sharpe, PBO/CSCV, block-permutation nulls, empirical
      p-values — cited implementations with property tests.
      - [x] **Block-permutation null + empirical p-values** (2026-07-31,
            D-0087): `crucible-funnel::stats::permutation`. The null is stated
            rather than implied — *returns are exchangeable at block scale L* —
            with the +1-corrected one-sided p-value, unevaluable draws counted
            rather than zero-filled, and an absent null failing its criterion.
            **M3's acceptance clause is met**: `planted_leak.rs` flipped from
            `Iterate` to `Kill`, by building the harness and by nothing else,
            with every gate before S3 still passing and asserted to pass. The
            converse control came first and caught a broken fixture on the way.
            Pinned hash `9fe41f6f5b3653e7`; the five existing hashes unmoved
      - [ ] Deflated Sharpe, PBO/CSCV — block B of `docs/plans/m3-full.md`

      **The planted defect they were measured against already existed**:
      `controls::LeakyZScore` (a full-sample z-score, §2.1's named lookahead),
      and `crucible-funnel/tests/planted_leak.rs` recorded that the gates
      returned `Iterate` for it. **That expectation flipped to `Kill` on
      2026-07-31** when the permutation null landed — the detector watched
      firing on a defect planted before it existed, which is what this line's
      acceptance test asked for
- [x] **Truncation-invariance harness** (2026-07-31, D-0088):
      `crucible-funnel::stats::truncation`. Decisions on `data[0..t]` compared
      **bit-for-bit** against the decisions `<= t` the full series made,
      truncating the END because that is the direction lookahead flows from. No
      tolerance knob — a one-tick difference after deleting the future IS the
      defect. Converse control first (`SmaCross`: 189 decisions, 0 divergences),
      then the detector (`LeakyZScore` refit per prefix: 4 divergences), then
      the third case (fit held fixed: 0 divergences, so the finding was the fit
      and not the shape). A mutation dropping the byte comparison **initially
      survived** — the detector was reading timestamps only — which is a missing
      control, now closed by a content-only leak fixture. Pinned hash
      `91b9ff5b9bbcdb25`; the six existing hashes unmoved
      - [ ] Wire both harnesses into CI as merge-blocking (§7 makes them so the
            day they land; they are green locally and not yet gated)
- [ ] Cross-instrument rhyme check (needs NQ/RTY archives from M1 tooling)
- [ ] Multi-instance pass + dataset semaphore, with criterion evidence

**Acceptance:** `crucible funnel configs/example-combo.toml` → scorecards +
registry rows, unattended; a deliberately-leaky test strategy is caught by
the permutation/truncation harnesses (negative-control test).
*First half met 2026-07-30:* `crucible funnel --config
configs/combo-smoke.toml` runs unattended to 24 registry rows, 6 verdicts and
a scorecard, killing all six combos of the null harness at S1 and exiting 5.
*Second half met 2026-07-31* (D-0087): the permutation null landed and
`planted_leak.rs` flipped from `Iterate` to `Kill` — the detector watched firing
on a defect planted before it existed, reached by building the harness and by no
other route. Its baseline had been measured rather than assumed, which is what
made the flip evidence instead of a claim. Every gate before S3 still passes and
the test asserts they do, so the file cannot silently stop measuring the
detector. The **truncation-invariance** half of the clause is still open.

## M4 — Calibration + write-up (~3–4 weeks)

> **BLOCKER ON THE RATES LEG, carried forward from M1 (D-0099).**
> **44 of 68 ZN contracts — 65 % of the rates leg — have never had spike
> detection run on them at all.** `qa`'s robust sigma is
> `1.4826 × median(|Δclose|)`, and ZN's 1/64 tick against quiet 1-minute bars
> makes that median exactly zero, so the check returned early. Until
> 2026-07-31 it returned **silently**: no line, which an automated sweep scored
> as zero spikes and a reader saw as nothing to report. The reporting half is
> fixed — every check now emits `SKIPPED — <reason>` — so the gap is visible,
> but it is **not closed**: those contracts are still unchecked, and the
> replacement estimator is unimplemented. Archive-wide, 47 contracts and
> **4,002,334 of 70,641,676 curated bars (5.7 %)** are affected.
> M4 calibrates against L1 and ZN is one of its instruments, so this is owed
> before any rates number is quoted.

- [ ] Spread/slippage calibration from archived L1 (measured half-spread by
      time-of-day replaces the hand-set 1 tick)
- [ ] MBO queue-position fill model prototype for limit orders; document
      divergence from `spread_cross`
- [ ] Macro-event overlay: static FOMC/CPI/NFP timestamp CSV → event studies
      (no scraping — D-0010)
- [ ] Paper trading skeleton: same `Strategy` trait on the Databento live
      feed, shadow fills, daily reconciliation report
- [ ] The write-up (mini-paper): methodology, one full case study
      idea→verdict, limitations — the quant-research portfolio piece
- [ ] README results section, scorecard screenshots, demo GIF

**Acceptance:** one strategy taken end-to-end idea → funnel → verdict →
(if it survives) paper trading, with the write-up telling the story honestly.

## Post-M4 (explicitly out of scope until here)

SSRN-style cross-sectional/rebalance strategies + multi-instrument portfolio;
ThetaData options integration; ETF datasets; GPU-accelerated screening; any
form of live order routing (never in scope for this project).
