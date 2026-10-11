# PumpSwap snapshot pipeline

The paper engine can now derive a quote-ready Pump state directly from raw
account bytes:

```text
Pool account
FeeConfig account
base vault account
quote vault account
base mint account
quote mint account
        |
        v
decode + mint safety
        |
        v
effective quote reserve
        |
        v
market cap in raw quote units
        |
        v
fee schedule / tier
        |
        v
creator-fee rules
        |
        v
PumpState
```

The caller still supplies two discovery classifications:

1. the fee schedule class (non-canonical / canonical SOL-like / stable /
   exotic);
2. the current GlobalConfig `creator_fee_configurable` gate.

Those are intentionally not guessed from pool bytes.

The returned `PumpSnapshotMeta` records the mint metadata, market cap and
resolved fees so paper-trading telemetry can explain exactly why a quote used
a specific tier.
