# Pump identity and automatic fee classification

The paper engine no longer needs a caller-supplied Pump fee schedule.

## Canonical pool

Pump's public fee documentation defines a canonical PumpSwap pool by comparing
`pool.creator` with `pumpPoolAuthorityPda(base_mint)`.

The authority is derived under the Pump program:

```text
program = 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
seeds   = ["pool-authority", base_mint]
```

A non-matching creator is classified as `NonCanonical` and uses flat fees.

## Canonical quote class

For a canonical pool:

- WSOL quote -> `CanonicalSolLike`
- quote mint present in Pump `Global.whitelisted_quote_mints` ->
  `CanonicalStable`
- anything else -> `CanonicalExotic`

This maps directly to the fee resolver's normal tiers, stable tiers, and exotic
flat-fee paths.

## Creator fee

The quote engine treats the fee already persisted in the pool as authoritative:

- no coin creator -> creator fee is zero
- `pool.creator_fee_bps == 0` -> schedule creator rate
- nonzero pool creator rate on a non-cashback coin -> pool override
- cashback coin -> schedule creator rate remains the cashback basis

Global creator-fee configurability and max values are decoded and retained as
diagnostics/configuration metadata. They are not used to retroactively change
a pool's persisted fee during quoting.

## Automatic snapshot

`assemble_pump_state_auto` now accepts raw:

- Pool
- FeeConfig
- Pump Global
- PumpSwap GlobalConfig
- base/quote vaults
- base/quote mints

and returns a quote-ready state plus the selected schedule, market cap,
resolved fees, and current global creator-fee configuration metadata.
