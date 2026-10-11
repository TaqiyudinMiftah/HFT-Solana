# Meteora DAMM v2

Stage 4 starts with the current Meteora DAMM v2 / cp-amm program.

Program: `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG`.

## Decoder source of truth

The decoder follows Meteora's current open-source program rather than an older
generated decoder. The current program declares a zero-copy Pool with
`Pool::INIT_SPACE == 1104`, `PoolFeesStruct::INIT_SPACE == 160`,
`BaseFeeStruct::INIT_SPACE == 40`, `DynamicFeeStruct::INIT_SPACE == 96`,
`PoolMetrics::INIT_SPACE == 80`, and `RewardInfo::INIT_SPACE == 192`.

The 8-byte Anchor discriminator makes a current Pool account 1112 bytes.

The Pool discriminator happens to be the same byte sequence used by a
PumpSwap Pool. Account routing must therefore use the owner program id, not
the discriminator alone.

## Why quoting is still gated

DAMM v2 is not a generic XYK pool. Depending on `collect_fee_mode`, the
program uses either concentrated sqrt-price/liquidity math or compounding
constant-product reserves. Fees can be collected on input or output and can
include base-fee modes, dynamic volatility fee, protocol/referral split,
claiming vs compounding split, and fee-version caps.

For `layout_version == 0`, the program reconstructs tracked token amounts
from its liquidity handler before upgrading the layout. Raw token-vault
balances are not a safe substitute.

This milestone therefore decodes all state required by exact quote math but
does not expose DAMM v2 as a routable `PoolState` yet. Route support is
enabled only after Rust quote math matches the official program/SDK.
