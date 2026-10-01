# PumpSwap fee resolution

The current Pump fee program selects the AMM fee schedule by canonical-pool
status and quote mint.

The core resolver models four already-classified paths:

- non-canonical pool -> `flat_fees`;
- canonical SOL-like quote -> market-cap `fee_tiers`;
- canonical listed stable quote -> `stable_fee_tiers`, falling back to
  `fee_tiers` for historical FeeConfig accounts;
- canonical exotic quote -> `exotic_flat_fees`, falling back to
  `flat_fees` while the exotic schedule is all zero.

## FeeConfig compatibility

The current SDK documents three account lengths:

- 2512: pre-stable layout
- 4073: stable tiers appended
- 4097: exotic flat fees appended

The decoder uses account length as the compatibility gate and decodes the
Borsh vectors with explicit count limits.

## Pool market cap

Market cap is calculated in raw quote base units using the pool's effective
quote reserve (raw + virtual) and base mint supply. Mayhem pools use the
protocol's fixed 1e15 raw-unit supply.

## Creator fee

After schedule selection:

- no coin creator -> creator fee is zero;
- when the global configurable-creator-fee gate is active, a nonzero
  pool-specific creator rate replaces only the schedule's creator rate;
- cashback pools do not apply the configurable creator override.

Canonical-pool PDA verification, quote-mint classification, and current
GlobalConfig decoding are separate discovery-state responsibilities.
