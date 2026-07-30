---
id: H-010
slug: vol-managed-oos-rebuttal
topic: vol-regime
grade: B
hypothesis_family: futures-vol-managed-exposure
status: backlog
created: 2026-07-30
---

# H-010 — The out-of-sample rebuttal: vol-managed portfolios as a real-time investor would have had them

## Citation

Scott Cederburg, Michael S. O'Doherty, Feifei Wang, Xuemin (Sterling) Yan,
**"On the performance of volatility-managed portfolios"**, *Journal of
Financial Economics* 138(1), 2020, 95–117.

- Publisher: <https://www.sciencedirect.com/science/article/abs/pii/S0304405X2030132X>
- SSRN: <https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3357038>
- RePEc: <https://econpapers.repec.org/article/eeejfinec/v_3a138_3ay_3a2020_3ai_3a1_3ap_3a95-117.htm>

Their stated findings: across a comprehensive set of **103 equity strategies**
evaluated for a **real-time investor**, volatility-managed portfolios **do not
systematically outperform** their corresponding unmanaged portfolios in direct
comparisons. The trading strategies implied by spanning regressions are **not
implementable in real time**, and reasonable out-of-sample versions generally
earn **lower certainty-equivalent returns and Sharpe ratios** than simple
investments in the original unmanaged portfolios — poor out-of-sample
performance stemming primarily from **structural instability in the underlying
spanning regressions**.

## Mechanism

This file has no profit mechanism. Its subject is a **methodological failure
mode**, and it is in the backlog because that failure mode is one Crucible is
specifically built to catch.

The mechanism being exposed is this: the standard evidence for volatility
management is a *spanning regression* — regress the managed portfolio's returns
on the unmanaged portfolio's and report the alpha. That regression is fit on the
whole sample. Its coefficients are therefore chosen with knowledge of the entire
period, and the "strategy" it implies is one no investor could have run, because
nobody knew those coefficients in 1970. When the same idea is implemented with
only the information available at each point in time, the coefficients turn out
to be unstable, and the performance disappears. In Crucible's vocabulary this is
**a full-sample statistic used inside a strategy** — the exact thing CLAUDE.md
§2.1 prohibits by name, alongside full-sample z-scores and "fit on everything
then backtest".

The losing side, if one insists on naming one, is the researcher.

## Signal in Crucible terms

Not a strategy. A **paired protocol** attached to H-009, plus a test of the
engine's own guarantees.

- **The pairing:** every H-009 result must be produced twice — once by the
  full-sample method that generates the published-style result, and once by a
  strictly point-in-time method where every parameter of the volatility model is
  estimated from a trailing window only. Both go in the same report, side by
  side, with the gap between them named.
- **The gap is the deliverable.** If the full-sample version shows a large
  Sharpe improvement and the point-in-time version shows none, we have measured
  the size of the lookahead in a widely published result — on futures, on our
  own data. That is a genuinely publishable observation for the M4 write-up and
  is worth more to this project than a positive result on H-009.
- **The parameter-stability report:** roll the volatility model's fitted
  parameters through the sample and report their trajectory. The rebuttal
  attributes the out-of-sample failure to structural instability; that
  attribution is directly checkable and nobody needs an equity curve to check
  it.

## Data

**Owned:** identical to H-009 — `ohlcv-1m`, seven parents, 2010-06-06 →
2026-07-28.

**Missing:** identical to H-009 (position sizing, realized-variance indicator,
daily grain), plus:

1. **A deliberate full-sample implementation, quarantined.** To measure the gap
   we must be able to *commit the sin on purpose* — compute a full-sample
   normalization and use it. Crucible's architecture makes this appropriately
   awkward, which is correct. It must live behind an explicit, loudly-named
   research-only path that can never be reached by a config, and any result it
   produces must be labelled as lookahead-contaminated in the output itself,
   not in a comment. If that cannot be done safely, this half of the test is
   dropped rather than smuggled in.
2. **The walk-forward runner**, which already exists (D-0062/D-0063) and is
   exactly the right instrument for the point-in-time half.

## Pre-registered kill criteria

The polarity is inverted: this file *expects* to find the effect disappear, and
the criteria are written so that finding is informative rather than a shrug.

- **Primary:** the difference between the full-sample-fit Sharpe improvement and
  the point-in-time Sharpe improvement must be reported with a block-bootstrap
  95 % CI, on at least **1,500 sessions** across at least **3 instruments**.
- **If the point-in-time version retains the improvement** (its CI excludes zero
  and its lower bound clears `min_oos_sharpe_after_costs = 0.5`), then the
  rebuttal does not transfer to futures, and H-009's verdict may proceed —
  **but only after** the M3 truncation-invariance harness has passed on it, on
  the general principle that a result contradicting a published rebuttal is an
  engine-bug alarm before it is a discovery (CLAUDE.md §7).
- **If the point-in-time version loses the improvement**, H-009 is **Killed**,
  and this file's output becomes a measured statement about how much of the
  published effect was full-sample fitting.
- **If the two versions are indistinguishable**, the honest conclusion is that
  our implementation had no meaningful full-sample content to begin with —
  which means the test was not testing what it claimed. That is a **void
  result**, not a pass, and it is recorded as such.
- **Parameter stability, judged separately:** roll the fitted parameters and
  report their dispersion. High instability corroborates the rebuttal's stated
  cause; low instability with a large performance gap means the gap has a
  *different* cause and we should find it before believing either paper.

**Trial accounting:** this file shares `futures-vol-managed-exposure` with
H-009 by design. Two papers, one idea, one trial counter.

## Honesty note

- **This file exists because the working prior says most papers are garbage, and
  the correct response is not cynicism but pairing.** H-009 is a highly-cited
  *Journal of Finance* paper; H-010 is a *Journal of Financial Economics* paper
  saying it does not survive real-time implementation. Both are peer-reviewed,
  both are credible, and the disagreement is exactly the kind of thing Crucible
  should be able to adjudicate on data neither team used.
- **Their data is 103 US equity strategies; ours is 7 futures.** We cannot
  reproduce their test. We can only ask whether the *same methodological gap*
  opens up on our asset class, which is a related but distinct question.
- **The deliberate-lookahead path is a real hazard.** Building a
  full-sample-normalization capability, even quarantined and labelled, adds a
  route by which lookahead could reach a result. That risk is why the
  requirement above is that it be unreachable from any config and
  self-labelling in its output — and why dropping this half of the test is an
  acceptable outcome. The project's prime invariant outranks the experiment.
- **Sample overlap:** their sample runs into the 2010s and overlaps roughly half
  of ours, but on instruments we do not hold, so the overlap is nominal.
- **CLAUDE.md §7's third-case rule applies directly here.** Two things
  disagreeing (full-sample vs point-in-time) tells you only that something
  differs. The parameter-stability trajectory is the third case that turns the
  difference into a diagnosis — it names *why* they differ. Without it this
  study reports a discrepancy and calls it an explanation.

## Triage grade

**B.** Same data and same missing code as H-009, plus a walk-forward runner that
already exists. It is graded and queued **with** H-009 and should never be run
without it — a vol-management result without its out-of-sample pair is exactly
the artifact this pairing exists to prevent.

---

## Changelog

Append-only. The registration above is never rewritten — a pre-registration
that gets edited after the fact is not one (README §1).

### 2026-07-30 — re-graded against the four grammar unlocks (D-0077…D-0080): **B → B**

**What closed.** The estimator half, with H-009: `stdev(period,
source="return")` (D-0080) and the `1d` grain (D-0077).

**What still blocks.** H-009's continuous-sizing gap, since this file is a
paired protocol and cannot outrun what it is paired to. Plus one that got
*sharper* rather than smaller, and is the more interesting half:

**The full-sample arm of this protocol is now structurally inexpressible in
TOML, by design.** The pairing requires every H-009 result to be produced
twice — once by the published-style full-sample method, once strictly
point-in-time — because the gap between them is the deliverable. D-0080 makes
every normalizer trailing-window only and states that no config can name a
full-sample variant. So the deliberate-lookahead arm cannot be written as a
config at all; it has to be a Rust control strategy, exactly like
`crucible-strategies::controls::LeakyZScore`, which exists for precisely this
purpose (CLAUDE.md §9).

That is the grammar refusing to express a lookahead, which is the grammar
working. It does mean this file's deliverable is a **control strategy plus a
config**, not two configs, and that is a change to how the work is shaped rather
than to how expensive it is. Recorded here so whoever takes the ticket does not
spend an afternoon trying to write the full-sample arm in TOML and conclude the
parser is broken.
