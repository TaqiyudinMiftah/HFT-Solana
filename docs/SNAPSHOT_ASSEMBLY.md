# Snapshot assembly

A quote state is assembled only from mutually compatible account bytes.

## Token accounts

SPL Token and Token-2022 share the same base account state. The base layout is
165 bytes; mint, owner, amount and account-state are decoded from that prefix.
Token-2022 extension bytes are not interpreted by the vault decoder.

## Mint safety gate

Reading a vault amount is not enough to make a Token-2022 pool quote-safe.
Transfer-fee, transfer-hook, or other quote-affecting mint behavior must be
resolved separately.

For that reason the snapshot API requires an explicit `MintQuoteSafety` for
both sides. Unknown/unsupported behavior is rejected instead of silently
producing an optimistic quote.

## PumpSwap snapshot

The assembler verifies vault mint identities, takes raw token balances, then
combines the pool's virtual quote reserve and already-resolved fee tier.

## Raydium CPMM snapshot

The assembler verifies vault mint identities, decodes the AmmConfig, and then
subtracts accrued protocol/fund/creator fee counters from raw vault balances
before creating the quote state.

The next milestone is mint-extension decoding so `MintQuoteSafety` can be
derived rather than supplied manually.
