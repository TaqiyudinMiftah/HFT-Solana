# Meteora DLMM

DLMM support is introduced behind the `meteora-dlmm` feature.

The implementation pins Meteora's official `dlmm-sdk` repository at
`576919e3e4368e542c402f000b4264724f7f23ec` and uses the Rust
`commons` quote engine locally. No RPC/API quote call is used in the hot
quote path.

## First milestone

The first milestone deliberately does not add DLMM to `PoolState` or the
cycle graph. It provides:

- official Pod decoders for `LbPair`, `BinArray`, and bitmap extension;
- an exact-input wrapper around `commons::quote_exact_in`;
- pinned upstream binary fixtures from pool
  `9t3EyC9FweyL7PBWvKz3mrXg8B9fwFc9SK3QxM4ENqhd`;
- parity tests in both X-to-Y and Y-to-X directions.

The fixture is the same one used by Meteora's own integration tests, including
liquidity distributed across multiple bins and limit orders.

## Why graph integration is gated

A quote-ready DLMM state needs more than the pair account:

- current `LbPair`;
- the relevant bin arrays in traversal order;
- optional bitmap extension when liquidity lies outside the internal bitmap;
- both mint accounts for Token-2022 transfer-fee handling;
- current timestamp, slot, and epoch.

Graph integration comes only after the fixture parity test is green and the
reactor can update the required bin-array set without publishing incomplete
state.
