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
and a list of candidate paths. For each paper opportunity the runner prints
either `PAPER_LANDING` with the selected candidate or
`PAPER_LANDING_SKIP` when no path clears all guards.

The probabilities and fees in `config/paper.example.json` are examples only,
not production estimates. They should be calibrated from observed delivery
telemetry and failed/successful transaction costs.

## Safety boundary

This layer only evaluates economics. It intentionally has no transaction
builder, signer, private key, Jito bundle sender, Helius Sender client, or
mainnet submission capability.
