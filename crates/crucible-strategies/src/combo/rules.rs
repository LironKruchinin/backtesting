//! The rule language: text in, a tiny AST out, booleans per bar.
//!
//! Parsing happens exactly once, when a config is loaded. Evaluation walks the
//! AST with no allocation, no string handling, and no map lookups — a combo
//! grid runs this per bar per combo, so anything that allocates here shows up
//! as a throughput ceiling in the funnel.
//!
//! ## Three rules that look like details and are not
//!
//! 1. **Evaluation never short-circuits.** `a and b` evaluates `b` even when
//!    `a` is false, because a `crosses_*` node has to see *every* bar to know
//!    what "previous" means. A skipped bar leaves it comparing against a
//!    reading from two bars ago, which is a lookahead-shaped bug (the signal
//!    fires on the wrong bar) with none of the obvious symptoms. The caller
//!    has the matching obligation: [`RuleSet`] must be evaluated in full on
//!    every bar, even bars whose outcome cannot matter.
//! 2. **Not-yet-warm is `None`, not `false`.** An unwarm slot makes the whole
//!    rule silent for that bar rather than false, because `not bb.upper < x`
//!    would otherwise read as *true* before the indicator has any opinion —
//!    a position taken on the absence of data.
//! 3. **A non-finite operand is also `None`.** A NaN in indicator space means
//!    something upstream already went wrong; comparisons against it are all
//!    false, which would silently look like a considered decision.

use std::fmt;

use crucible_core::events::Bar;

use crate::indicators::BollingerBands;

use super::spec::IndicatorKind;

/// A field of the current bar, in **signal space**.
///
/// Prices reach indicator space as points (`AdjustedPrice::as_points_f64`),
/// never as raw nanos (CLAUDE.md §2.3), and always through
/// `Bar::signal_*()` so that a rule comparing a price to an indicator compares
/// two numbers on the same scale. On an outright contract the two spaces
/// coincide; on a stitched continuous series they do not, and reading
/// `bar.close` here would compare a raw front-month price against an SMA of
/// back-adjusted ones — a difference the size of the cumulative roll gap,
/// silently.
///
/// **A rule comparing a price field to an absolute constant is level-sensitive
/// and is therefore not safe on a back-adjusted series** (D-0073): the level a
/// bar sits at depends on roll gaps that had not happened yet. That is why
/// `combo` and `walk-forward` refuse a continuous alias in this build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceField {
    /// Interval open.
    Open,
    /// Interval high.
    High,
    /// Interval low.
    Low,
    /// Interval close.
    Close,
}

impl PriceField {
    const fn name(self) -> &'static str {
        match self {
            PriceField::Open => "open",
            PriceField::High => "high",
            PriceField::Low => "low",
            PriceField::Close => "close",
        }
    }

    fn of(self, bar: &Bar) -> f64 {
        match self {
            PriceField::Open => bar.signal_open().as_points_f64(),
            PriceField::High => bar.signal_high().as_points_f64(),
            PriceField::Low => bar.signal_low().as_points_f64(),
            PriceField::Close => bar.signal_close().as_points_f64(),
        }
    }
}

/// Which output of an indicator slot a rule means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotField {
    /// The slot's single scalar output (SMA, EMA).
    Value,
    /// Bollinger middle band.
    Mid,
    /// Bollinger upper band.
    Upper,
    /// Bollinger lower band.
    Lower,
}

impl SlotField {
    const fn suffix(self) -> &'static str {
        match self {
            SlotField::Value => "",
            SlotField::Mid => ".mid",
            SlotField::Upper => ".upper",
            SlotField::Lower => ".lower",
        }
    }
}

/// One bar's reading from an indicator slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlotOut {
    /// A single number (SMA, EMA).
    Scalar(f64),
    /// Three numbers (Bollinger).
    Bands(BollingerBands),
}

impl SlotOut {
    fn field(self, field: SlotField) -> Option<f64> {
        match (self, field) {
            (SlotOut::Scalar(v), SlotField::Value) => Some(v),
            (SlotOut::Bands(b), SlotField::Mid) => Some(b.mid),
            (SlotOut::Bands(b), SlotField::Upper) => Some(b.upper),
            (SlotOut::Bands(b), SlotField::Lower) => Some(b.lower),
            // Unreachable: the parser refused a field the kind cannot produce.
            _ => None,
        }
    }
}

/// A leaf of a comparison.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    /// A literal number, in points (prices) or indicator units.
    Const(f64),
    /// A field of the bar being decided on.
    Price(PriceField),
    /// An indicator slot's output. `slot` indexes the spec's **sorted** slot
    /// list, so it is stable for every combo in a grid.
    Slot {
        /// Index into the sorted slot list.
        slot: usize,
        /// Which output.
        field: SlotField,
    },
}

impl Operand {
    fn value(&self, ctx: &EvalCtx<'_>) -> Option<f64> {
        let v = match self {
            Operand::Const(c) => *c,
            Operand::Price(p) => p.of(ctx.bar),
            Operand::Slot { slot, field } => {
                ctx.slots.get(*slot).copied().flatten()?.field(*field)?
            }
        };
        v.is_finite().then_some(v)
    }
}

/// Ordering comparison. Equality is deliberately absent — two `f64`s in
/// indicator space are equal by coincidence, never by intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

impl Cmp {
    const fn symbol(self) -> &'static str {
        match self {
            Cmp::Lt => "<",
            Cmp::Le => "<=",
            Cmp::Gt => ">",
            Cmp::Ge => ">=",
        }
    }

    fn holds(self, a: f64, b: f64) -> bool {
        match self {
            Cmp::Lt => a < b,
            Cmp::Le => a <= b,
            Cmp::Gt => a > b,
            Cmp::Ge => a >= b,
        }
    }
}

/// A parsed rule.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// An ordering comparison between two operands.
    Compare {
        /// Left operand.
        lhs: Operand,
        /// The comparison.
        op: Cmp,
        /// Right operand.
        rhs: Operand,
    },
    /// A crossover. True on the bar where the relation flips, which needs the
    /// previous bar's readings — hence `state`, an index into the strategy's
    /// crossover-state slab.
    Cross {
        /// Left operand.
        lhs: Operand,
        /// Right operand.
        rhs: Operand,
        /// `true` for `crosses_above`, `false` for `crosses_below`.
        above: bool,
        /// Index into the per-strategy crossover state.
        state: usize,
    },
    /// Conjunction.
    And(Box<Expr>, Box<Expr>),
    /// Disjunction.
    Or(Box<Expr>, Box<Expr>),
    /// Negation.
    Not(Box<Expr>),
}

/// What a rule can see when it is evaluated: this bar, and this bar's
/// indicator readings. Nothing else exists yet (CLAUDE.md §2.1).
pub struct EvalCtx<'a> {
    /// The bar being decided on.
    pub bar: &'a Bar,
    /// One entry per slot, in sorted-slot order; `None` until warm.
    pub slots: &'a [Option<SlotOut>],
}

impl Expr {
    /// Evaluates the rule, updating crossover state as it goes.
    ///
    /// `None` means "no opinion this bar" — an unwarm slot, a non-finite
    /// reading, or a crossover that has only seen one bar. Never treat it as
    /// `false`: see this module's docs.
    ///
    /// Deliberately does **not** short-circuit; every crossover node must see
    /// every bar.
    ///
    /// # Panics
    /// Panics if `cross_state` is shorter than the crossover-node count the
    /// [`RuleSet`] reported. That is a construction bug in the caller, not a
    /// config error, and it is provably impossible via [`RuleSet::new`].
    pub fn eval(&self, ctx: &EvalCtx<'_>, cross_state: &mut [Option<(f64, f64)>]) -> Option<bool> {
        match self {
            Expr::Compare { lhs, op, rhs } => {
                let (a, b) = (lhs.value(ctx), rhs.value(ctx));
                Some(op.holds(a?, b?))
            }
            Expr::Cross {
                lhs,
                rhs,
                above,
                state,
            } => {
                let slot = cross_state
                    .get_mut(*state)
                    .expect("INVARIANT: cross state is sized from RuleSet::cross_states()");
                let (Some(a), Some(b)) = (lhs.value(ctx), rhs.value(ctx)) else {
                    // Reset rather than keep a stale pair: comparing across a
                    // gap would fire the crossover on the wrong bar.
                    *slot = None;
                    return None;
                };
                let previous = slot.replace((a, b));
                let (pa, pb) = previous?;
                Some(if *above {
                    pa <= pb && a > b
                } else {
                    pa >= pb && a < b
                })
            }
            Expr::And(l, r) => {
                let (a, b) = (l.eval(ctx, cross_state), r.eval(ctx, cross_state));
                Some(a? && b?)
            }
            Expr::Or(l, r) => {
                let (a, b) = (l.eval(ctx, cross_state), r.eval(ctx, cross_state));
                Some(a? || b?)
            }
            Expr::Not(e) => e.eval(ctx, cross_state).map(|v| !v),
        }
    }

    /// Renders the expression in fully-parenthesized canonical form, so that
    /// whitespace, redundant parentheses and comments in the source text
    /// cannot change a config's identity (D-0012).
    fn render(&self, slots: &[SlotDecl], out: &mut String) {
        match self {
            Expr::Compare { lhs, op, rhs } => {
                out.push('(');
                render_operand(lhs, slots, out);
                out.push(' ');
                out.push_str(op.symbol());
                out.push(' ');
                render_operand(rhs, slots, out);
                out.push(')');
            }
            Expr::Cross {
                lhs, rhs, above, ..
            } => {
                out.push('(');
                render_operand(lhs, slots, out);
                out.push_str(if *above {
                    " crosses_above "
                } else {
                    " crosses_below "
                });
                render_operand(rhs, slots, out);
                out.push(')');
            }
            Expr::And(l, r) => render_binary(l, "and", r, slots, out),
            Expr::Or(l, r) => render_binary(l, "or", r, slots, out),
            Expr::Not(e) => {
                out.push_str("(not ");
                e.render(slots, out);
                out.push(')');
            }
        }
    }

    /// Crossover nodes under this expression. Used by the test that proves
    /// `RuleSet::cross_states` counts what it says it counts.
    #[cfg(test)]
    fn count_crosses(&self) -> usize {
        match self {
            Expr::Compare { .. } => 0,
            Expr::Cross { .. } => 1,
            Expr::And(l, r) | Expr::Or(l, r) => l.count_crosses() + r.count_crosses(),
            Expr::Not(e) => e.count_crosses(),
        }
    }
}

fn render_binary(l: &Expr, op: &str, r: &Expr, slots: &[SlotDecl], out: &mut String) {
    out.push('(');
    l.render(slots, out);
    out.push(' ');
    out.push_str(op);
    out.push(' ');
    r.render(slots, out);
    out.push(')');
}

fn render_operand(operand: &Operand, slots: &[SlotDecl], out: &mut String) {
    match operand {
        Operand::Const(c) => out.push_str(&canonical_f64(*c)),
        Operand::Price(p) => out.push_str(p.name()),
        Operand::Slot { slot, field } => {
            let name = slots
                .get(*slot)
                .map_or("<unknown>", |decl| decl.name.as_str());
            out.push_str(name);
            out.push_str(field.suffix());
        }
    }
}

/// Renders an `f64` as the shortest decimal that round-trips.
///
/// This is identity-critical: the string goes into the canonical form that
/// gets hashed (D-0012), so the rendering has to be the same on every machine
/// and every toolchain. Rust's `{:?}` for floats is specified to produce the
/// shortest round-tripping representation, which is that guarantee — and
/// `canonical_f64_is_pinned` in this module's tests is what notices if it ever
/// stops being true.
pub(crate) fn canonical_f64(x: f64) -> String {
    format!("{x:?}")
}

/// The four rules, parsed.
///
/// Absent rules are absent, not always-false: a config with no `enter_short`
/// simply never goes short.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleSet {
    /// Opens a long when true and the position is flat.
    pub enter_long: Option<Expr>,
    /// Closes a long when true and the position is long.
    pub exit_long: Option<Expr>,
    /// Opens a short when true and the position is flat.
    pub enter_short: Option<Expr>,
    /// Closes a short when true and the position is short.
    pub exit_short: Option<Expr>,
    cross_states: usize,
}

/// The four rules as written in a config.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleSource {
    /// `enter_long = "..."`
    pub enter_long: Option<String>,
    /// `exit_long = "..."`
    pub exit_long: Option<String>,
    /// `enter_short = "..."`
    pub enter_short: Option<String>,
    /// `exit_short = "..."`
    pub exit_short: Option<String>,
}

impl RuleSource {
    /// True when nothing was declared at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.enter_long.is_none()
            && self.exit_long.is_none()
            && self.enter_short.is_none()
            && self.exit_short.is_none()
    }
}

/// A slot as the parser sees it: a name a rule may reference, and the kind
/// that decides which fields exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotDecl {
    /// Slot name, as written in the config.
    pub name: String,
    /// Which indicator fills it.
    pub kind: IndicatorKind,
}

impl RuleSet {
    /// Parses all four rules against a slot table.
    ///
    /// # Errors
    /// [`RuleError`] naming the rule, the position in it, and what was wrong.
    /// Every failure is a config bug: an unknown slot, a field the slot's kind
    /// does not produce, a stray token.
    pub fn new(
        source: &RuleSource,
        slots: &[SlotDecl],
    ) -> Result<RuleSet, (&'static str, RuleError)> {
        let mut crosses = 0usize;
        let mut one = |name: &'static str,
                       text: &Option<String>|
         -> Result<Option<Expr>, (&'static str, RuleError)> {
            match text {
                None => Ok(None),
                Some(text) => {
                    let expr = parse(text, slots, &mut crosses).map_err(|e| (name, e))?;
                    Ok(Some(expr))
                }
            }
        };
        let enter_long = one("enter_long", &source.enter_long)?;
        let exit_long = one("exit_long", &source.exit_long)?;
        let enter_short = one("enter_short", &source.enter_short)?;
        let exit_short = one("exit_short", &source.exit_short)?;
        Ok(RuleSet {
            enter_long,
            exit_long,
            enter_short,
            exit_short,
            cross_states: crosses,
        })
    }

    /// How many crossover nodes need previous-bar state.
    #[must_use]
    pub const fn cross_states(&self) -> usize {
        self.cross_states
    }

    /// True if any rule uses a crossover, which costs one extra warmup bar:
    /// a crossover has no opinion until it has two readings.
    #[must_use]
    pub const fn uses_crossover(&self) -> bool {
        self.cross_states > 0
    }

    /// Renders the rules in canonical form, one line each, omitted rules
    /// omitted (D-0012).
    pub(crate) fn render(&self, slots: &[SlotDecl], out: &mut String) {
        for (name, rule) in [
            ("enter_long", &self.enter_long),
            ("exit_long", &self.exit_long),
            ("enter_short", &self.enter_short),
            ("exit_short", &self.exit_short),
        ] {
            if let Some(expr) = rule {
                out.push_str("rule ");
                out.push_str(name);
                out.push(' ');
                expr.render(slots, out);
                out.push('\n');
            }
        }
    }

    #[cfg(test)]
    fn total_crosses(&self) -> usize {
        [
            &self.enter_long,
            &self.exit_long,
            &self.enter_short,
            &self.exit_short,
        ]
        .iter()
        .filter_map(|r| r.as_ref())
        .map(Expr::count_crosses)
        .sum()
    }
}

/// A rule that could not be parsed or resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleError {
    /// Byte offset into the rule text where the trouble starts.
    pub position: usize,
    /// What went wrong, in the operator's terms.
    pub message: String,
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at offset {}: {}", self.position, self.message)
    }
}

impl std::error::Error for RuleError {}

// ---------------------------------------------------------------- tokenizer

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Number(f64),
    Cmp(Cmp),
    Dot,
    LParen,
    RParen,
}

impl Tok {
    fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("{s:?}"),
            Tok::Number(n) => canonical_f64(*n),
            Tok::Cmp(c) => c.symbol().to_owned(),
            Tok::Dot => ".".to_owned(),
            Tok::LParen => "(".to_owned(),
            Tok::RParen => ")".to_owned(),
        }
    }
}

/// Words a slot may not be named, because a rule already uses them.
pub(crate) const RESERVED: &[&str] = &[
    "open",
    "high",
    "low",
    "close",
    "and",
    "or",
    "not",
    "crosses_above",
    "crosses_below",
];

fn tokenize(text: &str) -> Result<Vec<(Tok, usize)>, RuleError> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let err = |position: usize, message: &str| RuleError {
        position,
        message: message.to_owned(),
    };
    while i < bytes.len() {
        let start = i;
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'(' => {
                out.push((Tok::LParen, start));
                i += 1;
            }
            b')' => {
                out.push((Tok::RParen, start));
                i += 1;
            }
            b'.' if !bytes.get(i + 1).is_some_and(u8::is_ascii_digit) => {
                out.push((Tok::Dot, start));
                i += 1;
            }
            b'<' | b'>' => {
                let two = bytes.get(i + 1) == Some(&b'=');
                let cmp = match (c, two) {
                    (b'<', false) => Cmp::Lt,
                    (b'<', true) => Cmp::Le,
                    (b'>', false) => Cmp::Gt,
                    _ => Cmp::Ge,
                };
                out.push((Tok::Cmp(cmp), start));
                i += if two { 2 } else { 1 };
            }
            b'=' => {
                return Err(err(
                    start,
                    "'=' is not a rule operator. Equality between two floats in indicator \
                     space is a coincidence, never an intent; use <, <=, > or >=",
                ));
            }
            b'-' | b'0'..=b'9' => {
                i += 1;
                let mut seen_dot = false;
                while i < bytes.len() {
                    match bytes[i] {
                        b'0'..=b'9' => i += 1,
                        b'.' if !seen_dot => {
                            seen_dot = true;
                            i += 1;
                        }
                        _ => break,
                    }
                }
                let text = &text[start..i];
                let value: f64 = text.parse().map_err(|_| {
                    err(
                        start,
                        &format!("{text:?} is not a number (exponents are not accepted)"),
                    )
                })?;
                if !value.is_finite() {
                    return Err(err(start, &format!("{text:?} is not a finite number")));
                }
                out.push((Tok::Number(value), start));
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                out.push((Tok::Ident(text[start..i].to_owned()), start));
            }
            _ => {
                return Err(err(
                    start,
                    &format!("unexpected character {:?}", char::from(c)),
                ));
            }
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------- parser

struct Parser<'a> {
    toks: Vec<(Tok, usize)>,
    at: usize,
    end: usize,
    slots: &'a [SlotDecl],
    crosses: &'a mut usize,
}

fn parse(text: &str, slots: &[SlotDecl], crosses: &mut usize) -> Result<Expr, RuleError> {
    let toks = tokenize(text)?;
    if toks.is_empty() {
        return Err(RuleError {
            position: 0,
            message: "the rule is empty; omit it entirely rather than declaring nothing".to_owned(),
        });
    }
    let mut parser = Parser {
        toks,
        at: 0,
        end: text.len(),
        slots,
        crosses,
    };
    let expr = parser.expr()?;
    if let Some((tok, pos)) = parser.peek() {
        let (tok, pos) = (tok.describe(), pos);
        return Err(RuleError {
            position: pos,
            message: format!("unexpected {tok} after a complete expression"),
        });
    }
    Ok(expr)
}

impl Parser<'_> {
    fn peek(&self) -> Option<(&Tok, usize)> {
        self.toks.get(self.at).map(|(t, p)| (t, *p))
    }

    fn position(&self) -> usize {
        self.toks.get(self.at).map_or(self.end, |(_, p)| *p)
    }

    fn fail<T>(&self, message: impl Into<String>) -> Result<T, RuleError> {
        Err(RuleError {
            position: self.position(),
            message: message.into(),
        })
    }

    /// Consumes the next token if it is the keyword `word`.
    fn eat_keyword(&mut self, word: &str) -> bool {
        if let Some((Tok::Ident(s), _)) = self.peek()
            && s == word
        {
            self.at += 1;
            return true;
        }
        false
    }

    fn expr(&mut self) -> Result<Expr, RuleError> {
        let mut left = self.conjunction()?;
        while self.eat_keyword("or") {
            let right = self.conjunction()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn conjunction(&mut self) -> Result<Expr, RuleError> {
        let mut left = self.unary()?;
        while self.eat_keyword("and") {
            let right = self.unary()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, RuleError> {
        if self.eat_keyword("not") {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        if let Some((Tok::LParen, _)) = self.peek() {
            self.at += 1;
            let inner = self.expr()?;
            match self.peek() {
                Some((Tok::RParen, _)) => {
                    self.at += 1;
                    return Ok(inner);
                }
                _ => return self.fail("expected ')'"),
            }
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Result<Expr, RuleError> {
        let lhs = self.operand()?;
        match self.peek() {
            Some((Tok::Cmp(op), _)) => {
                let op = *op;
                self.at += 1;
                let rhs = self.operand()?;
                Ok(Expr::Compare { lhs, op, rhs })
            }
            Some((Tok::Ident(word), _)) if word == "crosses_above" || word == "crosses_below" => {
                let above = word == "crosses_above";
                self.at += 1;
                let rhs = self.operand()?;
                let state = *self.crosses;
                *self.crosses += 1;
                Ok(Expr::Cross {
                    lhs,
                    rhs,
                    above,
                    state,
                })
            }
            _ => self.fail(
                "expected a comparison (<, <=, >, >=, crosses_above, crosses_below) — a rule \
                 is a condition, not a value",
            ),
        }
    }

    fn operand(&mut self) -> Result<Operand, RuleError> {
        let Some((tok, pos)) = self.peek() else {
            return self.fail("expected a number, a price field, or an indicator slot");
        };
        match tok.clone() {
            Tok::Number(n) => {
                self.at += 1;
                Ok(Operand::Const(n))
            }
            Tok::Ident(name) => {
                self.at += 1;
                self.resolve_ident(&name, pos)
            }
            other => Err(RuleError {
                position: pos,
                message: format!(
                    "expected a number, a price field, or an indicator slot, found {}",
                    other.describe()
                ),
            }),
        }
    }

    fn resolve_ident(&mut self, name: &str, pos: usize) -> Result<Operand, RuleError> {
        let field = if let Some((Tok::Dot, _)) = self.peek() {
            self.at += 1;
            match self.peek() {
                Some((Tok::Ident(f), _)) => {
                    let f = f.clone();
                    self.at += 1;
                    Some(f)
                }
                _ => return self.fail("expected a field name after '.'"),
            }
        } else {
            None
        };

        if let Some(price) = price_field(name) {
            return match field {
                None => Ok(Operand::Price(price)),
                Some(f) => Err(RuleError {
                    position: pos,
                    message: format!("{name} is a price, and has no field {f:?}"),
                }),
            };
        }
        if RESERVED.contains(&name) {
            return Err(RuleError {
                position: pos,
                message: format!("{name:?} is an operator word, not a value"),
            });
        }

        let Some(index) = self.slots.iter().position(|s| s.name == name) else {
            let known = self
                .slots
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(RuleError {
                position: pos,
                message: format!("unknown indicator slot {name:?}; declared slots are: {known}"),
            });
        };
        let kind = self.slots[index].kind;
        match kind.field(field.as_deref()) {
            Some(field) => Ok(Operand::Slot { slot: index, field }),
            None => Err(RuleError {
                position: pos,
                message: match field {
                    Some(f) => format!(
                        "{name} is a {} and has no field {f:?}; it produces {}",
                        kind.name(),
                        kind.fields_help()
                    ),
                    None => format!(
                        "{name} is a {} and has no single value; it produces {}",
                        kind.name(),
                        kind.fields_help()
                    ),
                },
            }),
        }
    }
}

fn price_field(name: &str) -> Option<PriceField> {
    match name {
        "open" => Some(PriceField::Open),
        "high" => Some(PriceField::High),
        "low" => Some(PriceField::Low),
        "close" => Some(PriceField::Close),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::prelude::*;

    fn slots() -> Vec<SlotDecl> {
        vec![
            SlotDecl {
                name: "bb".to_owned(),
                kind: IndicatorKind::Bollinger,
            },
            SlotDecl {
                name: "trend".to_owned(),
                kind: IndicatorKind::Ema,
            },
        ]
    }

    fn parsed(text: &str) -> Expr {
        let mut crosses = 0;
        parse(text, &slots(), &mut crosses).expect("valid rule")
    }

    fn rejected(text: &str) -> RuleError {
        let mut crosses = 0;
        parse(text, &slots(), &mut crosses).expect_err("should have been refused")
    }

    fn rendered(text: &str) -> String {
        let mut out = String::new();
        parsed(text).render(&slots(), &mut out);
        out
    }

    fn bar(close: f64, high: f64) -> Bar {
        Bar {
            instrument: InstrumentId::new("SYN:TEST"),
            tf: TimeFrame::M1,
            ts_open: Ts(0),
            open: Price::from_points_f64_lossy(close),
            high: Price::from_points_f64_lossy(high),
            low: Price::from_points_f64_lossy(close),
            close: Price::from_points_f64_lossy(close),
            volume: 1,
            signal_offset: Price::ZERO,
        }
    }

    /// A price operand is read in **signal space**, like every indicator it
    /// gets compared against (D-0073).
    ///
    /// Mixing the two is the failure this guards: `close crosses_above sma_20`
    /// on a stitched series would compare a raw front-month price with a
    /// moving average of back-adjusted ones, and the two disagree by the
    /// cumulative roll gap — hundreds of ES points, which is a rule that fires
    /// on the roll table rather than on the market.
    #[test]
    fn a_price_operand_is_read_in_signal_space() {
        let mut b = bar(100.0, 101.0);
        b.signal_offset = Price::from_points(20);
        // Traded at 100/101, sits at 120/121 in signal space.
        assert!((PriceField::Close.of(&b) - 120.0).abs() < 1e-9);
        assert!((PriceField::High.of(&b) - 121.0).abs() < 1e-9);
        assert!((PriceField::Open.of(&b) - 120.0).abs() < 1e-9);
        assert!((PriceField::Low.of(&b) - 120.0).abs() < 1e-9);
        // And with no offset the two spaces coincide, so the numbers above are
        // the offset arriving rather than the fixture.
        let plain = bar(100.0, 101.0);
        assert!((PriceField::Close.of(&plain) - 100.0).abs() < 1e-9);
    }

    /// `and` binds tighter than `or`: `a or b and c` is `a or (b and c)`.
    #[test]
    fn and_binds_tighter_than_or() {
        assert_eq!(
            rendered("close < 1 or close < 2 and close < 3"),
            "((close < 1.0) or ((close < 2.0) and (close < 3.0)))"
        );
        assert_eq!(
            rendered("(close < 1 or close < 2) and close < 3"),
            "(((close < 1.0) or (close < 2.0)) and (close < 3.0))"
        );
    }

    #[test]
    fn not_binds_tighter_than_and() {
        assert_eq!(
            rendered("not close < 1 and close < 2"),
            "((not (close < 1.0)) and (close < 2.0))"
        );
    }

    /// Redundant parentheses and whitespace must not survive into the
    /// canonical form, or two identical configs would hash differently.
    #[test]
    fn rendering_normalizes_away_layout() {
        assert_eq!(
            rendered("  ((close   <  bb.lower))  "),
            rendered("close < bb.lower")
        );
    }

    #[test]
    fn slot_fields_are_checked_against_the_kind() {
        assert!(rejected("close < bb").message.contains("no single value"));
        assert!(
            rejected("close < trend.upper")
                .message
                .contains("no field \"upper\"")
        );
        assert!(
            rejected("close < nope")
                .message
                .contains("unknown indicator")
        );
        assert_eq!(
            parsed("close < bb.lower"),
            Expr::Compare {
                lhs: Operand::Price(PriceField::Close),
                op: Cmp::Lt,
                rhs: Operand::Slot {
                    slot: 0,
                    field: SlotField::Lower
                },
            }
        );
    }

    #[test]
    fn equality_and_junk_are_refused_with_a_position() {
        let e = rejected("close == bb.mid");
        assert_eq!(e.position, 6);
        assert!(e.message.contains("coincidence"));

        let e = rejected("close < bb.mid extra");
        assert_eq!(e.position, 15);
        assert!(e.message.contains("unexpected"));

        assert!(rejected("close").message.contains("expected a comparison"));
        assert!(rejected("(close < 1").message.contains("expected ')'"));
        assert!(rejected("close < 1e3").message.contains("unexpected"));
        assert!(
            rejected("close & 1")
                .message
                .contains("unexpected character")
        );
    }

    #[test]
    fn a_slot_may_not_be_named_after_an_operator_word() {
        // The spec rejects such a slot at construction; the parser also has to
        // refuse the reference, because a rule is parsed before a slot table
        // is proven clean in some call orders.
        assert!(rejected("close < and").message.contains("operator word"));
    }

    /// Hand-derived. `fast` = close, `slow` = 10 (a constant stands in for the
    /// second series so the arithmetic is visible):
    ///
    /// | bar | close | prev vs 10 | now vs 10 | crosses_above |
    /// |-----|-------|-----------|-----------|---------------|
    /// | 0   | 9     | —         | below     | None (no prev)|
    /// | 1   | 10    | below     | equal     | false         |
    /// | 2   | 11    | equal     | above     | **true**      |
    /// | 3   | 12    | above     | above     | false         |
    /// | 4   | 8     | above     | below     | false         |
    #[test]
    fn crossover_needs_two_readings_and_fires_once() {
        let expr = parsed("close crosses_above 10");
        let mut state = vec![None; 1];
        let mut fired = Vec::new();
        for (i, close) in [9.0, 10.0, 11.0, 12.0, 8.0].into_iter().enumerate() {
            let b = bar(close, close);
            let ctx = EvalCtx {
                bar: &b,
                slots: &[],
            };
            fired.push(expr.eval(&ctx, &mut state));
            let _ = i;
        }
        assert_eq!(
            fired,
            vec![None, Some(false), Some(true), Some(false), Some(false)]
        );
    }

    /// The counterpart: `crosses_below` fires on the 8, and only there.
    #[test]
    fn crossover_below_is_the_mirror() {
        let expr = parsed("close crosses_below 10");
        let mut state = vec![None; 1];
        let fired: Vec<_> = [9.0, 10.0, 11.0, 12.0, 8.0]
            .into_iter()
            .map(|close| {
                let b = bar(close, close);
                expr.eval(
                    &EvalCtx {
                        bar: &b,
                        slots: &[],
                    },
                    &mut state,
                )
            })
            .collect();
        assert_eq!(
            fired,
            vec![None, Some(false), Some(false), Some(false), Some(true)]
        );
    }

    /// The reason evaluation must not short-circuit: a crossover under a false
    /// `and` still has to see the bar. With short-circuiting, bar 2's reading
    /// would be missed and the cross would be detected a bar late.
    #[test]
    fn a_false_conjunct_still_advances_the_crossover() {
        // `high > 1000` is false on every bar below, so a short-circuiting
        // `and` would never evaluate the crossover at all.
        let expr = parsed("high > 1000 and close crosses_above 10");
        let mut state = vec![None; 1];
        for close in [9.0, 10.0] {
            let b = bar(close, close);
            let _ = expr.eval(
                &EvalCtx {
                    bar: &b,
                    slots: &[],
                },
                &mut state,
            );
        }
        // Having seen both bars, the crossover node holds (10, 10).
        assert_eq!(state[0], Some((10.0, 10.0)));
    }

    #[test]
    fn unwarm_slots_are_silent_rather_than_false() {
        let expr = parsed("not close < bb.mid");
        let b = bar(1.0, 1.0);
        let mut state = Vec::new();
        assert_eq!(
            expr.eval(
                &EvalCtx {
                    bar: &b,
                    slots: &[None, None]
                },
                &mut state
            ),
            None
        );
        let warm = [
            Some(SlotOut::Bands(BollingerBands {
                mid: 5.0,
                upper: 6.0,
                lower: 4.0,
            })),
            None,
        ];
        assert_eq!(
            expr.eval(
                &EvalCtx {
                    bar: &b,
                    slots: &warm
                },
                &mut state
            ),
            Some(false)
        );
    }

    #[test]
    fn crossover_states_are_numbered_across_all_four_rules() {
        let source = RuleSource {
            enter_long: Some("close crosses_above trend".to_owned()),
            exit_long: Some("close crosses_below trend".to_owned()),
            enter_short: None,
            exit_short: Some("close < bb.mid".to_owned()),
        };
        let set = RuleSet::new(&source, &slots()).expect("valid rules");
        assert_eq!(set.cross_states(), 2);
        assert_eq!(set.total_crosses(), 2);
        assert!(set.uses_crossover());
    }

    /// D-0012 depends on this rendering being the same everywhere, forever.
    #[test]
    fn canonical_f64_is_pinned() {
        assert_eq!(canonical_f64(1.5), "1.5");
        assert_eq!(canonical_f64(2.0), "2.0");
        assert_eq!(canonical_f64(0.1), "0.1");
        assert_eq!(canonical_f64(100.0), "100.0");
        assert_eq!(canonical_f64(-0.25), "-0.25");
        // The value 0.1 + 0.2 famously is not 0.3, and the canonical form must
        // say so rather than round it into agreement.
        assert_eq!(canonical_f64(0.1 + 0.2), "0.30000000000000004");
    }
}
