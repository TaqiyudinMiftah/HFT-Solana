# Account decoding

The hot path does not depend on Anchor or JSON. It decodes the fields required
for quoting directly from account bytes.

## PumpSwap Pool

The decoder supports historical account lengths by treating appended fields as
zero/false when they are absent. Current fields used by the quote engine:

- base and quote mint/vault addresses;
- coin creator presence;
- mayhem flag;
- cashback flag;
- virtual quote reserves;
- configurable creator fee fields;
- holder-reward flag.

Vault token amounts arrive separately from the SPL/Token-2022 token accounts.

## Raydium CPMM

Raydium PoolState is packed. The quote reserve is **not** the raw token-vault
amount. The program removes accrued protocol, fund, and creator fee counters
before running curve math. `vault_amounts_without_fees` mirrors that rule.

The AmmConfig decoder extracts trade/protocol/fund/creator rates. The
PoolState decoder extracts the per-pool creator-fee enable flag and fee-side
mode.

Token-2022 transfer-fee extension handling remains a separate milestone.
