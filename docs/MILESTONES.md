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
- [ ] `crucible pull`: Databento batch download → `raw/` (`.dbn.zst`), each
      slice recorded through `Catalog::append`; `Catalog::coverage` decides
      what is actually requested, so nothing is paid for twice
      - [x] Quote path (`ingest::{plan,quote}`, 2026-07-24): coverage-subtracted
            month-aligned planning, live dataset-range clipping, per-window
            `get_cost`/`get_billable_size`, exact nano-USD spending gate,
            metered-vs-billed entitlement check. Spends nothing; no caller for
            `BatchProvider::submit` yet
      - [ ] Execute path: submit → poll → download → verify → append, with a
            crash-resumable job journal outside `raw/`
      - [ ] `ManifestRecord.symbols` = requested key ∪ raw symbols observed in
            the delivered DBN metadata (the assumption the validation slice
            exists to prove)
      - [ ] CLI wiring: `clap`, `--execute`, `--max-cost-usd`, exit codes
- [ ] Bootstrap pulls: 16y `ohlcv-1s`/`ohlcv-1m` + `definition` for the
      starting symbol set (ES first; NQ/RTY when cross-instrument checks land)
- [ ] Monthly archival job for the rolling L1/L3 windows — `trades`, `tbbo`,
      `mbo`; `mbp-10` deliberately excluded (D-0023). Documented cron, later
      automated; runs `--max-cost-usd 0.00` so it refuses rather than bills
      if the entitlement lapses
- [ ] `crucible transcode`: DBN → curated Parquet, partitioned, versioned
- [ ] `ParquetBarFeed` implementing `Feed` (mmap'd, availability-ordered)
- [ ] Session calendar v1 (Globex sessions, holidays, `bars_per_year`)
- [ ] Continuous contracts v1: volume-roll table + back-adjust at load;
      signals on adjusted, PnL on tradeable prices
- [ ] Data QA report: gaps, zero-volume runs, price spikes, DST boundaries

**Acceptance:** one command takes a fresh machine (with API key) to a
validated local ES archive; the demo strategy runs on real ES 1m bars.

## M2 — Engine hardening + combos (~3–4 weeks)

- [ ] Warmup alignment: funnel-controlled eval windows so every grid combo
      scores on identical bars (CLAUDE.md §2.6)
- [ ] Walk-forward runner: anchored + rolling folds over a `Feed`
- [ ] Config-driven combo strategies (`crucible-strategies::combo` spec):
      TOML → indicator graph + rule AST; deny-unknown-fields
- [ ] Stops/targets with worst-case intrabar ordering; flag path-sensitive
      results in outputs
- [ ] Golden tests vs an external reference (NautilusTrader or hand-audited
      runs) on identical data — the "why should anyone trust your engine"
      answer
- [ ] Indicator numerics: replace rolling sums with rebased/Welford updates
- [ ] Criterion benches: bars/sec single-run baseline on the 5800X3D

**Acceptance:** a combo defined purely in TOML backtests on real ES data
with reproducible, externally-cross-checked results.

## M3 — Funnel + statistics (~3–4 weeks)

The quant-research payload. Specs live in `crucible-funnel` module docs.

- [ ] Grid expansion + blake3 config identity; combo-count guardrails
- [ ] DuckDB registry: runs, metrics, hypotheses, trials, verdicts;
      insert-before-run; dedup/resume
- [ ] Rayon scheduler: run-level parallelism, dataset semaphore,
      multi-instance pass (one data replay feeding K combo instances)
- [ ] Stages S0–S3 with pre-registered kill criteria from config
- [ ] Stats: deflated Sharpe, PBO/CSCV, block-permutation nulls, empirical
      p-values — cited implementations with property tests
- [ ] Truncation-invariance harness in CI (sampled cut points)
- [ ] HTML scorecards with the mandatory honesty box
- [ ] Cross-instrument rhyme check (needs NQ/RTY archives from M1 tooling)

**Acceptance:** `crucible funnel configs/example-combo.toml` → scorecards +
registry rows, unattended; a deliberately-leaky test strategy is caught by
the permutation/truncation harnesses (negative-control test).

## M4 — Calibration + write-up (~3–4 weeks)

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
