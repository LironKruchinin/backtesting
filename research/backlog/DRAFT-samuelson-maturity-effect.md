---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: samuelson-maturity-effect
topic: term-structure-roll-yield
grade: B
hypothesis_family: commodity-samuelson-maturity-effect
status: draft
blocked_on: a time-to-expiry / contract-age operand, OR a criterion that reads the per-fold table: `walk-forward` already slices a contract's life into ordered folds, so the comparison is PRINTED but nothing machine-checks a trend across it
created: 2026-08-06
doi: 10.20944/preprints202505.2487.v1
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — The maturity effect: variance rising into expiry

> **This is a DRAFT, not a registration.** Nothing here has been run and
> nothing here is a recommendation. It was built from index metadata — title,
> venue, year, and the abstract the API returned — by `research/intake`;
> **the paper itself has not been read**. Promote it into `research/backlog/`
> by hand, after reading, or delete it.
>
> **The kill criteria below are PROPOSALS**, marked `criteria_status:
> proposed` in the front matter. A proposal is not a pre-registration: it
> becomes one when Liron approves it, by name, and the file is promoted. The
> marking is what lets a later reader tell a criterion someone committed to
> from a number a drafter suggested.

## Citation

Roza Galeeva. *In Pursuit of Samuelson for Commodity Futures: How to Parameterize and Calibrate the Term Structure of Volatilities*.
venue unrecorded, 2025.
DOI `10.20944/preprints202505.2487.v1`. <https://doi.org/10.20944/preprints202505.2487.v1>
Retrieved from the crossref API on 2026-08-06.

The author treats the rise in a futures contract's price variability as delivery nears as something to be parameterised and calibrated rather than merely noted, proposes a decaying form for the instantaneous variance, fits it to roughly fifteen years of energy futures, and reports that the fit stays inside its own statistical error except during one crisis stretch.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.20944/preprints202505.2487.v1':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

The economics are unusually clean for this batch. A contract's price has to converge on a delivery-date state, and the closer that date is, the less time remains for a new fact to be diluted by everything that might happen afterwards — so each arriving fact moves the near contract more than the far one. That is a statement about how information is absorbed, not about anyone's mistake, which is precisely why no losing side can be named: there is no trade in it. Selling the rise requires options this archive does not hold; sizing against it requires continuous position sizing the grammar does not have; and the pattern is so thoroughly documented that the front contract's higher variability is already priced into every margin requirement in the market. This registration is worth writing as a falsifiable property of our own data, not as a strategy, and the draft should be read that way.

## Signal in Crucible terms

- Instruments: single CL and GC contracts, four-digit keys, one config each. The property is about a single contract's own life, which is the rare case where the one-instrument restriction is not a handicap.
- Timeframe: `1h` or `1d`, aggregated on read; the effect is measured over months of a contract's life, not over minutes.
- Feature: `stdev(period, source = 'return')` is expressible and is exactly the right trailing statistic. What is missing is the other operand — days remaining to the contract's expiry — which no config can name.
- The measurement arm is nearly reachable by a different route: `walk-forward` already cuts a contract's replayable span into ordered folds and prints per-fold detail by grid index, so the dispersion of successive folds is already on the page. Nothing machine-checks whether it trends upward, and no criterion field expresses a monotonicity.
- Rule as it would be written for the strategy arm: widen or stand aside as expiry nears, e.g. `exit_long: days_to_expiry < 10`. That operand does not exist, and the archive's expiry data would have to reach the grammar through the D-0071 caller-side pattern rather than through a calendar dependency in the engine.

## Data

- Owned: every CL and GC contract, `ohlcv-1m`, 2010-06-06 to 2026-07-28, curated. This is a large number of independent contract lives, which is unusually good for a hypothesis of this shape.
- Owned: contract expiries, resolvable from the archived definition records under the `max(ts_recv)` rule with the availability filter D-0090 already built. The awkward part of a time-to-expiry feature is therefore already solved somewhere else in the tree.
- Not owned: Brent and natural gas, two of the three markets the paper calibrates on.
- Not owned: options or implied-volatility series, so the paper's implied-side extrapolation — the application it argues actually matters — has no counterpart here.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- The measurement arm is not registrable today, and the draft states that rather than dressing it up: no criterion field expresses 'later folds show greater dispersion than earlier folds', so there is nothing for the funnel to check. Registering a criterion no field can evaluate would be a pre-registration in name only.
- For the strategy arm — `min_oos_sessions = 250`, basis: one contract's replayable span is roughly sixty sessions, so this cannot be met before pooling and the run is correctly killed for sample adequacy.
- `min_oos_trades = 100` — basis: an expiry-conditioned filter fires once per contract life, so the trade count only becomes meaningful across many contracts.
- `min_oos_sharpe_after_costs = 0.40` — basis: standing aside near expiry is a low-turnover overlay, so a modest floor after costs is a real hurdle rather than a formality.
- `kill_if_dead_at_ticks = 1.0` — basis: at low turnover an edge that cannot clear the assumed one-tick half-spread is an artefact of the assumption rather than a finding.
- `require_controls_beaten = true` — basis: avoiding the most volatile stretch of a contract's life reduces dispersion mechanically, so the matched random-entry control is what separates risk reduction from an edge. This is the gate expected to kill the strategy arm.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- This is a preprint on a posting service with no peer review attached, dated 2025. The grading rests on index metadata; nobody here has read it.
- The paper's own stated purpose is derivative valuation and risk assessment, not trading. Converting it into a rule is our decision, and any result belongs to us and not to the paper.
- Two of the three calibration markets are absent from the archive, and the implied-volatility application that motivates the work is entirely unreachable without options data.
- The maturity effect is one of the oldest documented regularities in futures markets, which cuts both ways: it is unlikely to be a data-mining artefact, and it is equally unlikely to be a source of unexploited profit.
- Confounding is severe for the strategy arm: a contract's final weeks are also when liquidity migrates to the next contract, so anything measured near expiry mixes the information effect with a liquidity effect, and `half_spread_ticks = 1` is a fixed assumption that cannot represent a widening book (D-0120).

## Triage grade

**B.** B, and the closest thing in this batch to nearly-answerable. Two routes close it. A days-to-expiry operand supplied caller-side in the D-0071 pattern, reading the expiry machinery that already exists under D-0090; or a criterion that machine-reads a trend across the per-fold table the walk-forward already prints. The second is cheaper and is the more generally useful build, since a per-fold trend gate would serve every hypothesis about drift over a contract's life.
