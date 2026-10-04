# Landing probability calibration

The probability calibrator is an offline/paper-only component. It converts
labeled delivery outcomes into deterministic provider success probabilities.

## Outcome definition

A success means:

```text
delivery path landed AND the arbitrage committed
```

A landed transaction that reaches the arb program but reverts its final
profitability guard is therefore labeled as a failure for this probability.

This matches the semantics used by `LandingCandidate.success_probability_bps`.

## Integer smoothing

The calibrator uses integer pseudo-counts rather than floating point.

With prior successes `a`, prior failures `b`, observed successes `s`, and
observed failures `f`:

```text
p_success_bps =
    round(10000 * (s + a) / (s + f + a + b))
```

The default Laplace prior is `a = 1, b = 1`, so an unseen provider starts at
5000 bps instead of 0 or 10000.

## Weighted replay

Historical observations can carry an integer weight. This allows a curated
dataset to represent repeated equivalent outcomes without duplicating rows.

Zero-weight observations are ignored.

## Provider isolation

Counts are maintained independently for:

- Direct
- Jito
- Helius Sender

Calibrating one provider cannot alter another provider's estimate.

## Runtime boundary

The module can replace only the candidate's `success_probability_bps`.
Priority fee, relay-tip policy, failure fee, and all profitability guards stay
unchanged.

The current paper runtime does not auto-learn from unlabeled opportunities.
A probability estimate should only be applied after ingesting explicit outcome
labels from replay or observed transaction results.
