# M3-full — the plan from here to a reachable `Graduate`

**Status:** plan, not code. Written 2026-07-31, after the S0 predictor seam
landed (D-0082 measurement, D-0085 caller). **Precedence:** `docs/MILESTONES.md`
remains the executable checklist; this file is the ordering and the reasoning
behind it, and it gets corrected when a block lands rather than kept in parallel.

## What M3 has, and what it owes

Landed: grid expansion and config identity, the append-only registry with
insert-before-run and void records (D-0074, D-0083), the rayon scheduler, S0/S1/S2
with pre-registered criteria, the two mandatory controls, the cost sweep,
account-evaluation *capture*, and HTML scorecards.

Owed: `crucible-funnel::stats` — still a spec in module docs — plus registry
pooling and the account-evaluation *evaluator*. Five blocks, in this order.

The order is not preference. **Block A is first because every other block's
numbers are worth less until it exists**: PBO and deflated Sharpe are statements
about overfitting computed *on the runs*, and §9's `LeakyZScore` entry is the
standing proof that a statistic computed on a leaked run cannot tell a leaked
edge from a real one. Until the harnesses can ask the different question, B..E
are honest arithmetic on possibly-dishonest inputs.

---

## Block A — permutation and truncation harnesses

**Consumes:** the replay path, `controls::LeakyZScore`, the seeded-walk null
harness, `rand_chacha` and the D-0064 seed lineage.

**Emits:** two merge-blocking suites (§7 makes them merge-blocking *the day they
land*), an empirical p-value per run against the permutation null, and a
truncation-invariance verdict. Both attach to the scorecard, which today renders
the permutation null as a **named hole** (§9) — the hole is what this fills.

- **Permutation null:** re-run the exact pipeline on block-shuffled real returns
  (block length ≳ strategy horizon, so autocorrelation survives the shuffle) and
  on seeded random walks. Two readings, and they are different questions: the
  real-data edge must **vanish** on nulls, and the null distribution supplies the
  empirical p-value.
- **Truncation invariance:** for sampled cut points `t`, decisions computed on
  `data[0..t]` must be **bit-identical** to decisions `≤ t` computed on the full
  series. This is the one that catches lookahead code review misses, and it is a
  determinism property, not a statistical one — so it asserts equality, not
  similarity.

**Pins/re-pins:** no existing hash. It adds one — a permutation-null hash over
`(config, seed lineage, block length, draw count)` — and existing gates must stay
byte-identical, because the harness observes runs rather than changing them.

**Acceptance criterion — and it is the milestone's, not a proxy for it:**
`crucible-funnel/tests/planted_leak.rs` flips from `Iterate` to `Kill` for
`LeakyZScore`. **That flip IS the acceptance test.** §9 is categorical: reaching
`Kill` by any other route — tightening a threshold, adding a suspicious-IC kill
to S0, editing the expectation — ticks the milestone with a lie in it. The
strategy is already checked in and already registered as uncaught, so there is
nothing to build for the clause except the harness.

**Negative control (§7, no quality exemption):**
1. The planted leak must be *watched firing*: `LeakyZScore` → `Kill`, with the
   record naming which harness caught it and at what p-value.
2. The **converse** control, which is the one that makes the first meaningful: a
   strategy with a *real* edge on a series with real structure must **survive**
   the same harness. A detector that kills everything is not a detector. Build
   the planted-edge fixture before trusting the planted-leak result.
3. A **mutation** control on the harness itself: shuffle with `L = 1` (destroying
   the autocorrelation the block length exists to preserve) and watch the p-value
   move. A harness whose block length does not change its answer is not blocking.

**Contradiction watch:** none. This block is what §9's `LeakyZScore` entry and
`docs/MILESTONES.md`'s acceptance clause both already promise.

---

## Block B — deflated Sharpe and PBO/CSCV

**Consumes:** Block A's p-values; the registry's **trial counts**, read through
`Registry::trials_for`, which excludes voided runs **by construction** (D-0083 —
a trial counts while at least one run charged to it still stands, and hash-gate
rows were withdrawn by appended `void` records rather than deleted). Also
`Registry::verdicts_standing` rather than `verdicts`, for the same reason: the
log records what happened, the statistics read what counts.

**Emits:** a deflated Sharpe per combo carrying its trial count, and a PBO from
CSCV over IS/OOS recombinations. Both onto the scorecard, replacing named holes.

- **Deflated Sharpe** (Bailey & López de Prado 2014): corrects an observed Sharpe
  for trial count, skew and kurtosis. **A headline Sharpe without its trial count
  is not reported anywhere** — that is already the registry module's contract.
- **PBO/CSCV** (Bailey, Borwein, López de Prado & Zhu 2015): partition into `S`
  blocks, evaluate all IS/OOS recombinations, measure how often the IS winner
  underperforms OOS.

**Pins/re-pins:** adds a stats hash over `(config, trial count, block count,
seed)`. Existing five unchanged.

**Acceptance criterion:** on the null harness, deflated Sharpe must fall *below*
the naive Sharpe by the amount the trial count implies, and PBO must be
**near 0.5** — a random-walk grid's IS winner should be a coin flip OOS. A PBO
near zero on noise means the CSCV split is leaking.

**Negative control:**
1. **Trial-count sensitivity, watched firing:** the same run scored with a
   trial count of 1 and of 24 must give materially different deflated Sharpes.
   A deflated Sharpe that ignores its denominator is decoration.
2. **The void must move it:** void a trial and re-read — the deflated Sharpe must
   *rise*, because the denominator fell. This is the test that proves D-0083's
   exclusion is real rather than cosmetic, and it belongs here rather than in the
   registry, because here is where the number is consumed.
3. **PBO on a planted overfit:** a grid whose best IS combo is deliberately fit to
   noise must produce PBO ≈ 1.

**Contradiction watch:** **flagged.** §4 pins "trial" as
`(config_hash, account_id, combo_index)`, so *every account is a trial*
(D-0067, and `ACCOUNT_EVAL_SPEC.md` §4.6 item 3 says so explicitly). Block E
evaluates **16 accounts**. If E runs before B is aware of it, a strategy's trial
count silently multiplies by up to sixteen and every deflated Sharpe drops
accordingly. That is *correct* — it is exactly what §4.6 wants — but it must be a
stated consequence rather than a surprise, and the scorecard must show the trial
count's composition (combos × accounts) rather than a bare integer.

**The sixteen account-trials are NOT independent, the raw count over-deflates,
and that is accepted deliberately.** Deflated Sharpe's trial count is a
multiple-testing correction, and its usual reading assumes trials that are at
least roughly independent searches. Sixteen accounts are not: they are **one
signal under sixteen risk overlays**. The same combo, the same bars, the same
fills — what differs is the drawdown rule, the ratchet basis and the profit
target. The *effective* number of independent trials is therefore much closer to
the combo count than to combos × 16, and correcting by the raw product deflates
harder than the statistics strictly justify.

We use the raw count anyway, and the reason is a direction rather than a
degree: **the preferred error is against the strategy.** An over-deflated Sharpe
makes a real edge harder to claim; an under-deflated one makes a spurious edge
easier to publish. This project's whole discipline is to prefer the first, and
it is the same asymmetry the registry module already states about trial counts
(wrong large costs power, wrong small flatters everything downstream).

Two rules fall out and are binding on the implementation:

- **The over-deflation is reported, not hidden.** The scorecard states the trial
  count's composition *and* that the account dimension is a risk overlay rather
  than an independent search, so a reader can see the correction is
  conservative rather than mistaking it for a precise one.
- **Nobody may "fix" this by dividing the account dimension out.** An effective-
  N estimator that shrinks the denominator is a change that makes results look
  better, and it needs its own decision entry, its own justification, and a
  control showing it does not resurrect a known-spurious edge. Until then the
  conservative count stands.

---

## Block C — registry pooling across contracts (unlock 5)

**Consumes:** the registry, the fold planner, curated per-contract partitions.

**Emits:** one pooled verdict over many contracts of a root (`ESH2024`,
`ESM2024`, …), with every contract charged as a trial.

**What it removes:** the single-contract verdict ceiling. Today a grade-A config
replays one contract's active life — roughly 60 sessions for ES — and
`research/backlog/README.md` §6.2 is blunt that **the A column produces no
verdicts until pooling lands**: no sample-adequacy criterion worth registering is
satisfiable at 60 sessions, so today's A-grade runs are guaranteed to be killed
at admission, correctly, by the machine.

**What sample-adequacy gating becomes when pooling exists** — this is the part
worth deciding before building:

- The admission check stops being a near-certain kill and becomes a **real**
  gate. `min_oos_sessions = 250` is satisfiable by pooling ~4–5 ES contracts, so
  for the first time the criterion discriminates rather than always firing.
- **Sessions must be counted as pooled, non-overlapping sessions**, and pooling
  must not double-count: two contracts trade the same calendar days, and the
  D-0062 argument against overlapping OOS windows applies with equal force here.
  Pooling ES and NQ over the same 250 sessions is **not** 500 independent
  sessions. The honest denominator is the count of distinct trading days, and
  cross-instrument breadth is a *separate* claim (the rhyme check), not extra `n`.
- **The floors do not come down.** H-007 and H-008 both register 200 trades /
  250 sessions and both say the floors come down "only when registry pooling
  supplies the sessions honestly, never to make a short run pass". Pooling is how
  they are met, not a reason to lower them.

**Pins/re-pins:** a pooled-run hash. The five existing gates are single-contract
and unchanged.

**Acceptance criterion:** a config pooling N contracts reports `N ×` the trials
of a single-contract run, pooled OOS sessions equal to the count of **distinct**
trading days across those contracts, and a verdict that no longer dies at
admission for a config that previously did.

**Negative control:**
1. **Double-count control, watched firing:** pool two contracts whose date ranges
   overlap and assert the session count is the *union*, not the sum. Plant the
   naive sum and watch it fail.
2. **Trial-count control:** pooling N contracts must charge N trials, not one —
   and the deflated Sharpe from Block B must fall accordingly.

**Contradiction watch:** **flagged.** `combo`/`walk-forward` refuse continuous
aliases (D-0076) because a grid expands rules it has not seen and a constant
comparison is unsafe on a back-adjusted series. Pooling is the sanctioned route
to a long sample and **must not** be implemented by quietly enabling `ES.v.0` for
grids. If a future design wants that, it supersedes D-0076 explicitly.

---

## Block D — `Graduate` becomes reachable

**Consumes:** A, B, and C. **Emits:** the third verdict, and the removal of the
`Iterate` ceiling from every report and scorecard that currently states it.

`Graduate` is pinned in §4 as "survived the full battery", and D-0075 makes the
ceiling `Iterate` *because the battery is missing*. So this block is not "enable
a flag" — it is the moment the definition is satisfiable. **Exactly these
conditions gate it**, and each names the control that proves the gate can fail:

| # | Condition | Proven falsifiable by |
|---|---|---|
| 1 | S0: score predicts — `\|IC\| ≥ min_abs_ic` **and** its bootstrap CI excludes zero at one horizon | the null harness, which reads `\|IC\| = 0.0378` and is killed (D-0085) |
| 2 | S1: profitable cost-free | the null harness, killed at S1 today |
| 3 | S2: OOS Sharpe clears its floor at the declared sweep level, **and** both mandatory controls are beaten | a combo that loses to the random-entry median — an absent control **fails** rather than passes (D-0075) |
| 4 | Admission: pooled trades and sessions clear their floors | Block C's double-count control |
| 5 | S3: deflated Sharpe positive **after** the trial-count correction | Block B's trial-count sensitivity control |
| 6 | S3: PBO below `max_pbo` | Block B's planted-overfit control (PBO ≈ 1) |
| 7 | S3: edge **vanishes** on the permutation null, at a declared p-value | Block A's planted-leak control, and its planted-*edge* converse |
| 8 | S3: truncation invariance holds — decisions on `data[0..t]` bit-identical to decisions `≤ t` on the full series | Block A's mutation control |
| 9 | S3: plateau, not spike — `require_plateau` over the declared axes | a config whose only good combo is isolated must fail it |
| 10 | Cross-instrument rhyme, where the hypothesis claims one | a result that appears on ES and nowhere else |

**The rule that makes this list load-bearing:** a condition with no control that
has been *watched failing* is not a gate, it is a formality. §7 has no quality
exemption, so `Graduate` does not become reachable until every row above has its
control in the record.

**Acceptance criterion:** the null harness still cannot graduate (it dies at S0),
and a **planted-good** fixture — a synthetic series with real, non-leaked
structure, sized to clear the sample floors — does graduate. Both, or the verdict
is untested in one direction.

**Contradiction watch:** **flagged, and the sharpest in this document.** D-0075
says the ceiling is `Iterate` and "do not 'enable' it by relaxing the stage
list". This block *removes* that ceiling and therefore **supersedes D-0075's
ceiling clause**.

**That requires an explicit superseding decision entry** — not a quiet edit to
the stage list, and not an inference from this plan. The entry must, at minimum:
cite **D-0075 by number**, name **which clause** it supersedes (the `Iterate`
ceiling, *not* the stage-refusal clause, which stays), state that the battery
D-0075 said was missing now exists, and list the ten conditions above as what
"survived the full battery" is now defined to mean. Every report and scorecard
currently *states* the ceiling in words; those strings are part of the contract
and change in the same commit as the entry. Landing the code without the entry
would leave §9's "can never print `GRADUATE`" reading as current guidance while
the build printed it — the exact contradiction 4297fbb had to repair once
already.

---

## Block E — the bootstrap account evaluator (`ACCOUNT_EVAL_SPEC.md` §4)

**Consumes:** the four captured series (D-0071: per-trading-day PnL, the intraday
unrealized-equity high-water summary, per-round-trip MAE/MFE, the worst-day
pair), and the 16 account configs in `configs/accounts/`.

**Emits:** per account — breach probability, P(pass evaluation), payout cadence,
and for `personal_*` risk of ruin rather than synthetic drawdown (§4.5).

- **Breach is a first-passage problem** (§4.2), not an end-of-period one: an
  account dies the first time the running drawdown crosses its threshold, so the
  question is about the *path*, which is why the high-water summary is retained
  per session and why §3.3.1 proves that summary is exactly sufficient.
- **Block bootstrap at `L = 20` trading days**, with the mandatory sweep over
  other lengths (§4.3). **`n < 10L` — 200 sessions at `L = 20` — is refused**,
  which is a hard interaction with Block C: most single-contract runs cannot be
  evaluated at all until pooling supplies the sessions.
- **The intraday high-water series stays an O(1) reducer** (D-0071). This block
  consumes the 56-bytes-a-session artifact and must not grow it into a retained
  per-bar series; `the_high_water_reducer_is_sixteen_bytes` and
  `a_day_record_is_fifty_six_bytes` exist to refuse exactly that.

**`reference_only` (§4.7):** `true` on the five `tpt_*` configs, because TPT's
own rules forbid automated trading of the funded account. **It is not an
exclusion** — a reference-only account is evaluated by the **full** battery,
because its rule *structure* is what measures strategy fragility, and TPT is the
most informative set here precisely because one firm supplies both ratchet bases
at an identical drawdown amount. Related and separate: `automation_policy` is
`forbidden` for `tpt_*`, `conditional` for `topstep_*`, and **`unknown`** for
`apex_*` and `personal_*` — and `unknown` is a value, never permission. Closing
it by inference is forbidden.

**Pins/re-pins:** an account-evaluation hash over
`(config, account_id, L, draws, seed)`. The five existing gates unchanged — this
block reads captured series and computes; it does not replay.

**Acceptance criterion:** `ACCOUNT_EVAL_SPEC.md` §5's control table, which is
already written and already numbered — including §5.8, whose planted control is
that on a series with **planted loss clustering** `L = 1` gives strictly *lower*
`p_breach` than `L = 20`, while on an i.i.d.-by-construction series the two agree
within Monte Carlo error. That is a two-sided control with the third case
attached, exactly §7's shape.

**Negative control:** §5's table in full; the dependence control above is the one
that proves the block length is doing work rather than being a parameter.

**Contradiction watch:** **flagged.** Every account is a trial (§4, D-0067), so
evaluating all 16 charges 16 trials per combo and deflates every Sharpe from
Block B accordingly. Correct per §4.6, and it means **Block E cannot be run
"just to look"** — there is no free peek at an account, and the config declares
the account before the run rather than a CLI flag choosing it at report time.

---

## Ordering summary

```
A  harnesses ......... unblocks the meaning of everything below
B  DSR + PBO ......... consumes A's p-values and the registry's trial counts
C  pooling ........... makes sample floors satisfiable, and E runnable at all
D  Graduate .......... consumes A+B+C; supersedes D-0075's ceiling clause
E  account evaluator .. needs C for n >= 10L; charges 16 trials into B
```

C before E is a hard dependency (`n < 10L` refusal). D last, because it is the
only block that *removes* a refusal, and it should remove it after the machinery
that justifies removing it exists — the same ordering rule D-0075 and D-0085
already applied to `s0`: the refusal lifts in the commit where the thing can
actually run, never earlier.
