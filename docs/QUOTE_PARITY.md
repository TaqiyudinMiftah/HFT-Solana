# Quote parity

Local quoting is allowed into the opportunity hot path only after exact
raw-integer parity has been demonstrated.

## PumpSwap

Current requirements encoded by the adapter:

- base side uses the raw base vault balance;
- quote side uses `raw_quote_reserve + virtual_quote_reserves`;
- fee amounts use integer ceiling division;
- exact-quote buys separate effective curve input from LP/protocol/creator fee;
- the current buy path applies the SDK's one-unit integer adjustment before
  constant-product output calculation;
- sells calculate gross quote output first, then subtract fees;
- cashback creator fee is tracked separately from immediate swap output.

`fixtures/pumpswap_known_swaps.json` contains real raw-unit reconciliation
fixtures. Tests require zero-unit error.

## Raydium CPMM

The adapter mirrors the current on-chain `CurveCalculator::swap_base_input`
semantics:

- trade fee is ceiling-divided;
- when creator fee is on input, trade + creator rate is charged together and
  the creator portion is split from that total;
- when creator fee is on output, the trade fee is charged on input and creator
  fee is ceiling-divided from curve output;
- protocol/fund rates split the trade fee and do not change user output;
- constant-product output uses integer floor division.

Token-2022 transfer fees are deliberately not yet included. Pools requiring
transfer-fee extensions must be rejected until that adapter lands.

## Cashback denomination

A cashback amount is only added directly to effective PnL when it is
denominated in the cycle's starting asset. Other cashback legs are marked as
unconverted and excluded from effective PnL until the conversion engine is
implemented. This mirrors the distinction observed between base cashback and
converted cashback in the wallet research.
