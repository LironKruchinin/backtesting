---
id: TODO(human) — allocate against research/backlog/ at promotion time
slug: london-fix-overnight-gold
topic: metals-lease-rates-carry
grade: A
hypothesis_family: gc-fix-to-fix-session-window
status: draft
created: 2026-08-07
doi: 10.2139/ssrn.6077836
source_api: crossref
harvested_from: crossref
accessed: 2026-08-06
criteria_status: proposed
---

# DRAFT — Holding gold between the London fixes, as a clock-band rule

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

Peter Bell. *&amp;nbsp;Arbitrage Trading Strategy in Gold Futures.*.
venue unrecorded, 2026.
DOI `10.2139/ssrn.6077836`. <https://doi.org/10.2139/ssrn.6077836>
Retrieved from the crossref API on 2026-08-06.

TODO(human) — the verbatim claim, quoted from the paper, no paraphrase creep.
Read from the index metadata: the author reports that holding gold from one London
fixing to the next — the overnight leg rather than the daytime one — produced
consistent gains over roughly a decade, and likens the pattern to what a band-based
mean-reversion rule extracts from a mean-reverting series.

**The abstract is deliberately not reproduced here** (D-0112). Read it in the
corpus record, or in the paper:

```bash
python - <<'PY'
import json
for line in open("research/intake/corpus/papers.jsonl", encoding="utf-8"):
    r = json.loads(line)
    if r["doi"] == '10.2139/ssrn.6077836':
        print(r["abstract"]); break
PY
```

The corpus is gitignored precisely so this repository carries no third-party
prose, and a source paper's own performance figures belong in the Honesty note
and nowhere else (`research/backlog/README.md` §1) — an abstract routinely
leads with one, so embedding it would put that figure in the Citation section
by construction.

## Mechanism

If a benchmark price is set at a fixed instant and a large, predictable quantity of
business is transacted at it, then flow around that instant is not information — it
is obligation. Funds that must mark or transact at the fix are price-takers by
mandate, and whoever is willing to hold the position through the interval between
fixes is being paid to absorb that. That is a real payer with a real reason to keep
paying, and it is the strongest thing this candidate has going for it. The weakest
thing is that the claim as stated is a decade-long directional hold on an asset
that rose over that decade, which is a description of gold in 2000–2010 as much as
of any fix effect — and the file's kill criteria are built around exactly that
confound rather than around the effect.

## Signal in Crucible terms

- One instrument, a single `GC` contract such as `GCZ2024`; `timeframes = ["1m"]`;
  raw contract, `spread_cross`.
- The two fixes are fixed London wall-clock instants, and the metals session opens
  at a fixed local instant, so each fix sits at a roughly constant
  `minutes_since_open` offset. That makes the whole rule two comparisons against
  constants:
  `enter_long = "minutes_since_open >= 960"` and
  `exit_long  = "minutes_since_open >= 690 and minutes_since_open < 960"`.
  The position is therefore held from the afternoon fix, through the close and the
  reopen, to the following morning fix — which is the paper's leg.
- The two constants are the whole hypothesis and they are declared before the run.
  They are **not** grid axes: sweeping them would turn a stated claim about two
  named instants into a search over the clock, which is a different and much less
  honest experiment.
- Second arm, under the same family: the paper's own analogy, a `bollinger` band on
  `close` with the same session gate, so that "the fix window pays" and "gold mean
  reverts and the window is incidental" are separated rather than conflated.
- Nothing here needs arithmetic between operands, an anchored price, a rolling
  extremum or a calendar predicate. That is why it is A while most of wave 2 is not.

## Data

- Owned: GC `ohlcv-1m` for 221 curated contracts, 2010-06-06 → 2026-07-28, and a
  bundled CME metals session calendar with eras (D-0089), which is what makes the
  clock readings real rather than assumed.
- Owned but assumed: `half_spread_ticks = 1`. GC has no `tbbo` in the archive and
  cannot acquire any (D-0120), and this rule pays one round trip per session, so
  the cost assumption is load-bearing.
- **The offset is an approximation and the size of the error is known.** The fixes
  are London-time events and the session open is Chicago-time, and the two
  jurisdictions change their clocks on different dates — so for roughly four weeks
  a year the fix sits sixty minutes away from where the constants put it. Those
  weeks are a known defect of the construction, not noise, and the run should
  report how many of its sessions fall in them.
- Not owned: the fixing prices themselves, and the volume transacted at them. The
  rule uses the *time* of the fix and never its price, which is what keeps it
  inside the grammar.

## Pre-registered kill criteria

**Proposed, not registered.** Every threshold below was chosen before any
equity curve exists, by someone who does not know the answer — which is the
half of pre-registration this file can honestly supply. The half it cannot is
Liron's approval, and until that is given these are suggestions with a basis
attached, not commitments.

- `min_oos_sessions = 250` — basis: one round trip per session means sessions are
  the sample, and a claim about a daily ritual needs a year of them. Not reachable
  on one contract's active life, so this is expected to kill the first run for
  sample size, correctly.
- `min_oos_trades = 200` — basis: the rule trades every session by construction, so
  a trade count far below the session count would mean the clock gate is not firing
  and the config is broken rather than the idea.
- `min_oos_sharpe_after_costs = 0.4` — basis: a fixed-clock hold is the simplest
  possible rule and has no parameter to have been fitted, so it should be held to a
  higher bar than a conditioned rule, not a lower one.
- `kill_if_dead_at_ticks = 1.0` — basis: one round trip per session at a fixed
  minute is a high-turnover construction against an assumed spread. If it dies at
  one tick it dies.
- **The buy-and-hold discriminator is the criterion that matters.** The rule is long
  a rising asset for most of every day, so it must beat buy-and-hold over the same
  window, not merely make money. A version that fails this has rediscovered gold's
  drift.
- The complement must also be run: long between the *morning* fix and the *afternoon*
  one — the daytime leg the paper says does not pay. If both legs clear the bar,
  the fix window is doing nothing and the result is the drift again.
- `require_controls_beaten = true` and `max_permutation_p = 0.05` — basis: a rule
  that is in the market on a fixed schedule is exactly the shape that beats nothing
  while looking respectable, and the matched random-entry median over sixteen draws
  is the control built for it.

No performance figure for Crucible appears anywhere in this file, in this
section or any other — the backlog's binding rule. The thresholds here are
floors a machine checks, not outcomes anyone anticipates.

## Honesty note

- An SSRN working paper by a single author, not refereed, describing a decade in
  which gold roughly quadrupled. The abstract's own framing — that the returns
  *resemble* what a band strategy extracts from a mean-reverting series — is an
  analogy offered in place of a test.
- The paper's own reported figures are theirs and are not restated here. None was
  produced under a fill model.
- Their window is roughly 2000–2010 and ours begins in 2010, so there is **no
  sample overlap at all**. Whatever this is, our archive tests it out of sample,
  which is the one genuinely attractive property of the candidate.
- The fixing mechanism itself changed inside our window: the process was reformed
  and re-administered in 2015 after a manipulation investigation, and several banks
  were penalised. A rule premised on predictable flow around a benchmark should be
  tested on both sides of that reform separately, because the reform's whole purpose
  was to remove the predictability.
- The DST mismatch is a real defect and is stated in the Data section rather than
  buried: for about four weeks a year the config is trading a window that is an hour
  away from the one it names.

## Triage grade

**A.** A. Two comparisons against constants and an optional band indicator, on one raw
contract at one timeframe — legal TOML today, with no new Rust and no new data.
Runnable is not answerable: one GC contract's active life is far short of
`min_oos_sessions = 250`, so the machine kills this for sample size until registry
pooling lands. It is also the candidate in this batch most likely to look good for
a bad reason, which is why the buy-and-hold and daytime-leg discriminators are
registered as criteria rather than as follow-ups.
