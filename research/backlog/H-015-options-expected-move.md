---
id: H-015
slug: options-expected-move
topic: options-context
grade: C
hypothesis_family: es-implied-move-context
status: backlog
created: 2026-07-30
---

# H-015 — The options-implied expected move as a conditioning feature

## Citation

The construct itself — "expected move" as the at-the-money straddle price, or as
a one-standard-deviation implied range over a horizon — is a **practitioner
convention, not an academic object**. It appears in broker education material
and options-analytics vendors, with varying conventions (the ATM straddle
directly, ~0.85 × the straddle, or ATM implied volatility scaled by √t). There
is no canonical paper defining it and none is cited here as though there were.

The academic content underneath it is the implied-vs-realized volatility
literature, which is substantial and refereed:

Bent J. Christensen, Nagpurnanand R. Prabhala, **"The relation between implied
and realized volatility"**, *Journal of Financial Economics* 50(2), 1998,
125–150.

- Publisher: <https://www.sciencedirect.com/science/article/pii/S0304405X98000348>
- RePEc: <https://econpapers.repec.org/RePEc:eee:jfinec:v:50:y:1998:i:2:p:125-150>
- PDF: <https://finance.martinsewell.com/stylized-facts/volatility/ChristensenPrabhala1998.pdf>

Their stated finding: implied volatility from at-the-money one-month OEX options
on the S&P 100 is an **unbiased and efficient forecast** of subsequently
realized index volatility in the post-1987-crash period, **outperforming past
realized volatility** and in some specifications subsuming its information
content. They attribute the difference from earlier, more pessimistic studies to
a longer sample, non-overlapping observations, and a regime shift around the
October 1987 crash.

See also H-011 (Bollerslev, Tauchen & Zhou 2009), which is the same raw
material — implied versus realized variance — used as a *return* predictor
rather than as a *volatility* forecast. The two files are deliberately separate:
this one makes no claim about direction.

## Mechanism

This file proposes a **feature, not a strategy**, and the distinction is the
whole point of it. The claim is narrow and well-supported: the options market
produces a forward-looking estimate of how far an index is likely to move over a
given horizon, and that estimate is better than the estimate you would form from
past realized volatility alone. It is better because option prices aggregate the
positioning of participants who are paying real money to be right about
volatility, and because it responds *immediately* to information a trailing
realized-volatility window only learns about after the fact — a scheduled
central-bank meeting three days out is in the option price today and in realized
volatility never, until it happens.

Who is on the losing side? For the *feature* the question does not arise, and
that is a feature of the file. Reading a forecast off a price is not a trade and
nobody pays for it. The moment this becomes a strategy — selling the expected
move, or trading its breach — the payer question returns immediately and is
answered in H-011: it is the hedger buying insurance, and harvesting their
premium means bearing the risk they are paying to shed.

## Signal in Crucible terms

- **Basket:** ES, conditioned on SPX-derived implied volatility. The index is
  the same; the instruments are not (see Honesty note).
- **Timeframe:** daily. The data we are acquiring is end-of-day, so a daily
  feature is the *only* honest granularity — an intraday expected move is not
  available and must not be interpolated from a daily one.
- **Feature 1 — expected move:** ATM implied volatility for the nearest
  expiry beyond the horizon, scaled to that horizon. The scaling convention
  (straddle-based vs σ√t) must be **pre-registered before the first run**; they
  are different numbers and picking the one that works afterwards is a free
  parameter.
- **Feature 2 — the realized/implied ratio:** the previous session's realized
  range divided by the expected move that was quoted for it. A rolling measure
  of whether the market has been over- or under-estimating its own movement.
- **Use — strictly as a conditioner**, in the same role H-004 plays:
  - a regime axis for other hypotheses (does H-008's short-horizon reversal
    behave differently on high-expected-move days?);
  - a reporting slice on every scorecard;
  - a sizing input, if and when the exposure-scaling layer of H-009 exists.
- **Explicitly not proposed here:** any rule that trades the expected move
  itself. That is a short-volatility strategy, it needs H-011's machinery and
  H-011's crisis gate, and it does not get to hide inside a file graded as a
  feature.

## Data

**Being acquired right now, and genuinely well-scoped.** `docs/THETADATA_PLAN.md`
records a **PROFESSIONAL** options entitlement at tick granularity with a
history floor of **2012-06-01**, covering nine roots including SPX, SPXW, VIX
and NDX, with all endpoints available including `greeks/*` and
`implied_volatility`. Tranche **T0** — `greeks/eod` above each root's greeks
floor, `eod` below it, and `open_interest` throughout, full span 2012-06-01 →
now — was in progress while this file was written. A useful incidental: index
levels themselves are unentitled, but `greeks/*` responses carry
`underlying_price`, so SPX/NDX/RUT/VIX levels arrive alongside the option data.

**Missing:**
1. **Any loader joining options data to futures bars.** ThetaData integration is
   **explicitly post-M4** (`docs/MILESTONES.md`). Nothing in the engine can read
   this data today.
2. **An availability rule, which does not exist yet and is the first design
   question, always** (CLAUDE.md §2.1). For a daily options snapshot: as known
   when? An EOD greeks record dated *D* reflects the close of *D* and cannot
   inform any decision before that close. Getting this wrong by one session is
   the same error `docs/DATA_PLAN.md` warns about for the Cboe files, and it
   would be worth roughly one session of lookahead on a volatility-conditioned
   strategy — which is more than enough to manufacture a result.
3. **Intraday implied volatility**, which would need tranche **T1** (1-minute
   quotes across the chain, estimated at ~1.75 TB CSV with an unmeasured
   compression ratio). Not acquired, and gated behind a measure-first rule.

**Known data-quality caveat already documented (D-0054):** ThetaData's `eod`
endpoint **duplicates every contract in older eras** — ratio exactly 2.000 on
sampled dates from 2014 through 2021-12-15, and **4×** on 2020-01-02 — becoming
clean from 2022-01-03. `open_interest` and `greeks/eod` are unaffected. Any
feature built from `eod` below the greeks floor must deduplicate, and must not
hardcode "two builds" because the 2020-01-02 counterexample forbids it. A
naive implementation double-counts a decade of open interest.

## Pre-registered kill criteria

Feature-shaped, because the file proposes a feature.

- **Gate −1 — the availability fixture, blocking.** No run is authorized until a
  hand-checked fixture asserts that an options record dated *D* is invisible to
  every decision made before *D*'s close. This gate is not about performance and
  cannot be waived by a good result.
- **Gate 0 — does it forecast better than what we already have?** The whole
  justification is that implied beats trailing realized. So test exactly that,
  with no trading: regress next-session realized range on (a) trailing realized
  volatility alone and (b) trailing realized plus the expected move.
  - The expected move must add explanatory power at the **5 %** level under a
    block bootstrap (block = 20 sessions), over at least **1,500 sessions**.
  - If it does not, the feature is **Killed** — we already compute realized
    volatility from 1-minute bars for free, and a feature that needs a vendor
    subscription to match something we own is not a feature.
- **Gate 1 — the conditioner must separate.** As in H-004: for at least one
  hypothesis in this backlog, out-of-sample performance must differ
  significantly between high- and low-expected-move buckets, in the same
  direction, on at least **2 of 3** of ES/NQ/RTY. Otherwise **Kill** as a
  conditioner; it may survive as a reporting slice.
- **Gate 2 — bucket population:** at least **250 sessions** in every bucket, with
  bucket boundaries set from a **trailing** window only. Full-sample quantiles
  are lookahead (CLAUDE.md §2.1) and would be an automatic void.
- **Gate 3 — the duplication control.** Before any result is believed, verify
  the deduplication against `greeks/eod` and `open_interest` on the sampled
  dates in D-0054, including 2020-01-02. A feature built on silently doubled
  data is void, not weakened.
- **Trial accounting:** conditioning an existing hypothesis on `k` buckets
  multiplies *that* hypothesis's trial count by `k`, charged to its own family.

## Honesty note

- **We would condition ES on SPX options.** Same underlying index, different
  instruments, and the futures–cash basis is itself a traded quantity with its
  own dynamics. This approximation must be stated on every result that uses the
  feature. A cleaner alternative exists in principle — options *on* ES futures
  are a CME product — but they are not in the ThetaData basket and are not in
  any acquisition plan.
- **The sample is bounded at 2012-06-01, not 2010-06-06.** Options-featured work
  is limited by ThetaData history, not by our sixteen-year bar archive
  (`docs/PROJECT_PLAN.md` §8 says exactly this). Roughly two years of our bar
  data can never carry this feature, and any comparison between a
  feature-conditioned result and an unconditioned one must run on the common
  sub-sample or it is comparing samples as well as methods.
- **VIX1D exists only from 2023 and daily SPX 0DTE only from 2022.** Any
  short-horizon expected-move variant has a three-to-four-year sample.
- **Christensen & Prabhala is a 1998 paper about 1980s–90s OEX options.** The
  options market has been transformed since — electronic quoting, a vastly
  larger and more sophisticated volatility-trading community, and the growth of
  0DTE. That implied volatility forecasts realized volatility is one of the more
  durable findings in the field and I expect it to hold, but the *magnitude* of
  its edge over trailing realized is very much a function of the era, and Gate 0
  is written to measure it on our data rather than assume it.
- **The practitioner convention is not one convention.** Straddle price,
  0.85 × straddle, and σ√t give materially different numbers. Pre-registering the
  choice matters more here than it looks.
- **This file is deliberately unambitious.** The mission asked for
  options-implied context *as a feature*, and the discipline of stopping there
  is what keeps it separable from H-011. The moment it becomes a strategy it
  inherits a risk premium, a captive payer, and a crisis gate — and it should
  inherit H-011's file, not this one's grade.

## Triage grade

**C.** The data is mid-acquisition with a real 2012+ span and a professional
entitlement, which is better than most C entries in this directory — but no
loader exists, ThetaData integration is explicitly post-M4, the availability
rule has not been designed, and a documented duplication bug sits across a
decade of the exact endpoint a daily feature would read. The cheapest useful
action is not to test it but to **write the availability rule down** while the
acquisition is still being designed, so the loader is built against it rather
than retrofitted to it.
