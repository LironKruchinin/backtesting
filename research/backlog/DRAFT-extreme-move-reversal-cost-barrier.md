---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: extreme-move-reversal-cost-barrier
topic: liquidity-provision-market-making
grade: A
hypothesis_family: reversal-after-extreme-bar
status: draft
created: 2026-08-07
doi: null
source_api: arxiv
harvested_from: arxiv
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Fading an extreme bar, and whether the spread eats the reversal

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

Adam G. Zawadowski, Gyorgy Andor, Janos Kertesz. *Short-term market reaction after extreme price changes of liquid stocks*.
arXiv q-fin, 2004.
**no DOI** (preprint). <http://arxiv.org/abs/cond-mat/0406696v1>
Retrieved from the arxiv API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: examining liquid US equities after large intraday
moves, the authors find prices reverse after both large falls and large rises, that
the result holds as their parameters are varied, and — the part that matters here —
that on one of the two venues the spread widened enough at the event to remove most
of what a contrarian could take, while on the other it did not, so the same
statistical reversal was worth something in one market and nothing in the other.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["title"].startswith('Short-term market reaction after extreme'):
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

A large move in one bar is usually somebody needing to be out, and being out is
worth paying for. Whoever takes the other side is providing immediacy and is paid
the reversal. That is the liquidity-provision story in its simplest form, and it
names its payer exactly: the forced seller. The paper's real contribution is the
second half — the compensation and the cost of collecting it move together, because
the spread widens for the same reason the reversal exists. This is the rare case
where a paper's own finding maps directly onto a mechanism this build already has:
the cost-sensitivity sweep (§2.4) is precisely the instrument that separates "there
is a reversal" from "there is a reversal you can have", and the paper says those are
different answers in different markets.

## Signal in Crucible terms

- One instrument, one timeframe, raw contract. Registered on the non-equity roots —
  `ZNM2024` and a single `6E` contract — because the paper's own markets are
  equities and this is an extension to markets it never looked at, not a
  replication.
- `[indicators.shock] kind = "zscore", period = [60, 120, 240], source = "return"` —
  a trailing standardization of the bar return, which is the closest legal
  expression of "this move is large relative to what this market has been doing".
- `enter_long  = "shock crosses_below -3.0"`, `exit_long = "shock crosses_above 0.0"`;
  mirrored for the short side. Both directions are registered because the paper
  reports reversal after moves of both signs, and testing one would be a
  half-experiment whose sign was chosen after the fact.
- Second arm under the same family: the same rule gated on `volume` above a declared
  threshold, so that "the move was large" and "the move was large and rushed" are
  separated. Volume is an operand (D-0079) and the threshold is per grain.
- No arithmetic, no anchored price, no rolling extremum, no calendar. Everything
  here is a trailing-window indicator and a comparison, which is why it is grade A.
- Deliberately **not** registered: a stop or target. The grammar cannot declare one
  (§2.1), and a reversal rule without a stop is a different risk profile from the
  paper's — stated here rather than discovered at S2.

## Data

- Owned: ZN and 6E `ohlcv-1m` and `ohlcv-1s`, 2010-06-06 → 2026-07-28, 68 and 149
  curated contracts.
- Owned: rates and FX session calendars with eras (D-0089), so an overnight-versus-
  session split of the results is available if wanted.
- Owned but assumed, and this is the crux: `half_spread_ticks = 1`. The paper's
  central finding is that the spread at the event decides the answer, and D-0120
  says the archive cannot measure the spread for either of these roots at any time,
  let alone conditionally at the moment of a shock. **The one number this hypothesis
  turns on is the one number this archive cannot supply.** That is stated here as
  the candidate's main weakness rather than left for the sweep to imply.
- The 0/0.5/1/2-tick sweep is the partial answer: it does not measure the spread,
  but it does say at which assumed spread the edge dies, and the paper's own result
  is exactly a statement of that form.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: a shock filter at three trailing sigmas fires
  rarely by construction, so a year of sessions is the minimum that produces a
  usable count. Not reachable on one contract.
- `min_oos_trades = 150` — basis: at one-minute grain a three-sigma return filter
  should fire often enough that a much smaller count means the threshold is picking
  up a handful of days rather than a behaviour.
- `min_oos_sharpe_after_costs = 0.3` — basis: deliberately below the shipped default,
  because the interesting output of this candidate is the *cost level at which it
  dies*, and a high bar at the costed stage would kill it before that number was
  produced.
- `kill_if_dead_at_ticks = 0.5` — basis: this is the registered discriminator and it
  is set tight on purpose. The paper's finding is that a widened spread removes the
  profit; a rule that survives only at zero ticks has reproduced the negative half
  of the paper, which is a result and should be recorded as one rather than as a
  failure.
- `require_controls_beaten = true` — basis: a mean-reversion rule that enters after
  large moves is entering at high volatility, which flatters raw dollar figures. The
  matched random-entry median over sixteen draws is the control that removes that.
- `max_permutation_p = 0.05` — basis: short-horizon reversal is the single most
  data-mined family in this whole directory, and a block permutation of the returns
  is the cheapest available defence.
- The two arms must be compared: if the volume gate makes no difference, the
  immediacy story has no support here and what remains is a plain volatility filter.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- Their market is US cash equities on two venues in the early 2000s. Ours is a
  Treasury note future and a euro future on CME between 2010 and 2026. Nothing about
  the sample overlaps, and the microstructure differs at every level — tick size,
  fragmentation, market-maker obligations, session length.
- The paper's own reported figures are theirs and are not restated here.
- Wave 1 already carries three reversal candidates — `eurusd-intraday-reversion`,
  `commodity-shock-efficiency` and `gold-oil-abnormal-return-day`. This one is kept
  as a separate candidate because its registered discriminator is the *cost level*
  rather than the reversal, and because it is registered on rates and FX rather than
  on energy and metals. If Liron judges that too close, the right resolution is to
  merge it into the family rather than to run both: a family that counts four
  variations of one idea as four ideas is exactly what the trial count and the
  overfitting battery exist to punish, and the `hypothesis_family` key here should
  then be shared with them.
- The instrument choice is deliberately the one that makes the test hardest: ZN is
  quiet and 6E is efficient, so if a reversal is going to fail to pay for its
  spread anywhere, it is here.

## Triage grade

**A.** A. A trailing z-score of returns, a volume comparison and four rules on one raw
contract — legal TOML today, no new Rust and no new data. Runnable is not
answerable: one ZN or 6E contract's active life falls well short of
`min_oos_sessions = 250`, so the machine kills it for sample size until registry
pooling lands. Its value even then is mostly the shape of its cost sweep rather than
its verdict.
