# Crucible

**A backtesting engine designed to reject strategies.**

Most backtesters are optimized to show you an equity curve going up. Crucible
is optimized to tell you, as fast and as honestly as possible, that your idea
doesn't work — because on the rare occasion it can't, that's evidence worth
having. Rust, event-driven, built on Databento CME futures data.

<!-- CI badge: add after first push
[![CI](https://github.com/CHANGEME/crucible/actions/workflows/ci.yml/badge.svg)](…)
-->

## Why this exists

The standard failure mode of strategy research is a pipeline that quietly
flatters the researcher: lookahead leaks, free fills, one in-sample split,
a thousand silent parameter trials behind one reported Sharpe. Crucible's
design inverts each of those defaults:

- **No lookahead by construction.** Every event carries an *availability
  time* (a 1-minute bar opening at 09:30 doesn't exist until 09:31); replay,
  decisions, and fills are ordered by it. Orders can never fill against the
  bar that triggered them.
- **Costs are named, never implied.** Fill models are explicit, versioned
  assumptions (`free_fills`, `spread_cross`, later an MBO-calibrated queue
  sim). Every result states the assumption that produced it, and every
  scorecard includes a cost-sensitivity sweep.
- **Ambiguity is counted, not resolved in your favour.** An OHLC bar does not
  say whether its high or its low printed first, so a stop and a target inside
  one bar could both have been hit. One named convention decides
  (`stop_first_intrabar`: the stop fills; a gap fills at the opening print,
  never at the level) and every result reports how many of its exits rested on
  that choice rather than on the data.
- **Trials are counted automatically.** A registry records every run of every
  hypothesis family; reported Sharpe ratios ship with their deflated
  counterpart (Bailey & López de Prado) and PBO estimates.
- **The null hypothesis is a first-class citizen.** Strategies are re-run on
  seeded random walks and block-shuffled data; performance that survives
  shuffling is treated as a bug detector, not a discovery.
- **Bit-exact determinism.** Same config + same data ⇒ identical results,
  enforced in CI on every commit.

## The funnel

Ideas flow through staged gates — cheap and optimistic first, expensive and
brutal last. Most ideas should die in seconds:

```mermaid
flowchart LR
    A[idea + config] --> S0[S0 signal triage<br/>predicts anything?]
    S0 -->|kill| G[(graveyard<br/>+ reason)]
    S0 --> S1[S1 coarse grid<br/>free fills]
    S1 -->|kill| G
    S1 --> S2[S2 walk-forward<br/>honest costs]
    S2 -->|kill| G
    S2 --> S3[S3 battery<br/>DSR · PBO · permutation<br/>plateaus · regimes]
    S3 -->|kill| G
    S3 --> V[Graduate:<br/>paper trading]
```

## Quick start

```bash
cargo run -p crucible-cli -- demo
```

runs the reference SMA-cross strategy over 100,000 seeded random-walk bars
under two fill models and prints the comparison. The punchline is the point:
the data is a random walk, so the honest line should lose — an engine change
that makes the demo profitable under costs introduced a bug, not an edge.

The demo needs no configuration at all. Real data does:

## Configuration

Two environment variables, neither with a default:

| Variable | Meaning |
|---|---|
| `DATABENTO_API_KEY` | Databento API key. Read from the environment by bin targets only, never passed as an argument or written to a config, manifest, or log. |
| `CRUCIBLE_DATA_DIR` | Archive root (`raw/`, `curated/`, `manifest.jsonl`). Must live **outside** the repo — it grows to 16 years of bars plus rolling L1/L2/L3 windows. |

Export them in your shell, or put them in a `.env` at the repo root — it is
gitignored, and a real environment variable always beats the file, so CI
secrets and one-off overrides keep working:

```dotenv
DATABENTO_API_KEY=db-your-key-here
CRUCIBLE_DATA_DIR=E:/crucible-data
```

**Windows paths:** a backslash starts an escape sequence in dotenv syntax.
Use forward slashes (`E:/crucible-data`) or single quotes
(`'E:\crucible-data'`); a bare `E:\crucible-data` is a parse error, and the
binary exits rather than starting up half-configured.

Check what actually resolved — presence and length for the key, never its
value:

```bash
cargo run -p crucible-cli -- env
```

## Acquiring data

`pull` downloads entitled Databento windows into the archive. It is a **dry
run by default**: it plans (subtracting what the manifest already records),
prices every window through free metadata endpoints, prints the total, and
exits without spending. Buying needs `--execute` *and* an explicit
`--max-cost-usd`, which is compared in integer nanodollars and refuses rather
than proceeding.

The Databento client is behind a non-default feature, so ordinary builds and
CI stay free of its dependency graph:

```bash
# Quote it. Costs nothing, can be run as often as you like.
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema ohlcv-1m --symbols ES.FUT \
       --start 2024-01-01 --end 2024-02-01

# Buy it, with a ceiling you chose.
cargo run -p crucible-cli --features databento -- \
  pull --dataset GLBX.MDP3 --schema ohlcv-1m --symbols ES.FUT \
       --start 2024-01-01 --end 2024-02-01 --execute --max-cost-usd 1.00

# Re-hash every archived file against the manifest.
cargo run -p crucible-cli -- verify
```

Re-running the same command is always safe: intents are identified
deterministically and reconciled against the vendor's own job list, so an
interrupted pull resumes and never buys a window twice. Exit codes: `0` done,
`2` usage, `3` refused to spend, `4` failed, `5` still in flight (re-run to
resume).

What to buy and what to refuse: [docs/DATA_PLAN.md](docs/DATA_PLAN.md). The
ordered procedure for the subscription month:
[docs/RUNBOOK_BLITZ.md](docs/RUNBOOK_BLITZ.md).

## Backtesting archived data

Raw DBN is what the vendor sold; **curated Parquet** is what the engine
replays. `transcode` converts one into the other — one file per instrument per
source window, six integer columns, and the source file's blake3 recorded in
the file's own metadata, so any result can name the exact bytes behind it.
Prices are `i64` nanopoints from Databento's wire format all the way to the
engine: there is no `f64` anywhere on the path.

```bash
# Build curated bars from everything in the manifest. Idempotent; --force rebuilds.
cargo run -p crucible-cli --features databento -- transcode

# Replay them. No vendor feature needed — this reads local Parquet.
cargo run -p crucible-cli -- backtest --instrument ESH2024 --timeframe 1m   --start 2024-01-01 --end 2024-02-01 --fast 20 --slow 50
```

Curated data is disposable: it is rebuilt from `raw/`, which the manifest
checksums, so deleting `curated/` is always safe. Every number `backtest`
prints comes with the assumptions that produced it — fill model, half-spread,
fee, bar count, date range, and the manifest id of the source.

The first real run, for calibration of expectations: SMA(20/50) on ESH4 over
January 2024, 30,167 one-minute bars, **−23.51 %** of capital under
`spread_cross`. With costs switched off it still loses **−5.21 %** — so there
was no edge to begin with, and paying the spread roughly quadrupled the
damage. That is the control group working exactly as intended.

## Workspace layout

| Crate | Role |
|---|---|
| `crucible-core` | Types, events, traits. Zero deps, zero I/O. |
| `crucible-data` | Databento ingest, immutable archive + manifest, calendars, continuous contracts, synthetic feeds. |
| `crucible-engine` | Deterministic single-threaded replay: portfolio, fill models, metrics. |
| `crucible-strategies` | Streaming indicators (SMA/EMA/Bollinger), strategies, config-driven combos. |
| `crucible-funnel` | Walk-forward folds, parallel scheduling, funnel stages, overfitting stats, trial registry, scorecards. |
| `crucible-cli` | `crucible` binary. |

Status: **M0 (skeleton) complete**, **M1 (data foundation) in progress** —
`crucible pull` acquires real data into a checksummed, append-only archive,
`crucible transcode` turns it into curated Parquet, and `crucible backtest`
replays it through the engine. Session calendars, continuous contracts, and
the data-QA report are what remain of M1.
Roadmap: [docs/MILESTONES.md](docs/MILESTONES.md).
Working conventions and invariants: [CLAUDE.md](CLAUDE.md). Decision log:
[docs/DECISIONS.md](docs/DECISIONS.md).

## Methodology references

Bailey & López de Prado, *The Deflated Sharpe Ratio* (2014) · Bailey,
Borwein, López de Prado & Zhu, *The Probability of Backtest Overfitting*
(2015) · White, *A Reality Check for Data Snooping* (2000) · López de Prado,
*Advances in Financial Machine Learning* (2018), chs. 11–14.

## License

MIT
