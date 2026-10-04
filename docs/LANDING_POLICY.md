# Paper landing policy

The landing model is deliberately separate from transaction submission.

It compares candidate delivery paths using integer lamport economics:

```text
net_if_landed =
    expected_effective_profit
    - base_fee
    - priority_fee
    - relay_tip
```

and probability-weighted expected value:

```text
EV =
    p_success * net_if_landed
    - (1 - p_success) * failure_fee
```

All probabilities use basis points; no floating point is used.

## Providers

The paper model recognizes:

- Direct
- Jito
- Helius Sender

These names are classifications only. There is no live RPC sender, bundle
submission, private key, or transaction signing path in this module.

## Guard rails

A landing candidate is rejected when:

- success probability is outside 0..=10000 bps;
- relay tip exceeds the configured share of expected effective profit;
- retained profit after successful landing is below the configured minimum;
- probability-weighted EV is below the configured minimum.

Among valid candidates, the selector maximizes expected value. Ties prefer
higher landing probability, then lower success cost, then deterministic
provider order.

## Runtime configuration

The optional `landing` section in the paper JSON config contains one policy
and a list of candidate paths. A candidate can use either a fixed `relay_tip`
or an adaptive `relay_tip_share_bps` derived from the opportunity's expected
effective profit. Adaptive tips can be clamped with `minimum_relay_tip` and
`maximum_relay_tip`.

For each paper opportunity the runner resolves the actual tip, then prints
either `PAPER_LANDING` with the selected candidate or
`PAPER_LANDING_SKIP` when no path clears all guards.

The probabilities and fees in `config/paper.example.json` are examples only,
not production estimates. They should be calibrated from observed delivery
telemetry and failed/successful transaction costs.

## Safety boundary

This layer only evaluates economics. It intentionally has no transaction
builder, signer, private key, Jito bundle sender, Helius Sender client, or
mainnet submission capability.


## Adaptive tip model

When `relay_tip_share_bps` is configured:

```text
raw_tip = expected_effective_profit * relay_tip_share_bps / 10000
relay_tip = clamp(raw_tip, minimum_relay_tip, maximum_relay_tip)
```

The global `max_tip_share_bps` guard is still applied after the clamp. This
keeps the dynamic model bounded even when the configured minimum tip dominates
a small opportunity.

Fixed `relay_tip` remains supported for deterministic replay and backward
compatibility. If `relay_tip_share_bps` is present, it takes precedence over
the fixed value.
