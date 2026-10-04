# Token-2022 mint safety

Token-2022 preserves the base Mint/Account state and stores extensions in TLV
data after the base layout. The engine uses the current
`spl-token-2022-interface` parser rather than maintaining a second manual TLV
implementation.

## Conservative gate

The paper engine rejects mints whose extensions can change raw transfer amount,
require extra transfer accounts/program execution, or make transferability
state-dependent.

Currently blocked examples:

- TransferFeeConfig
- TransferHook
- NonTransferable
- Pausable
- ConfidentialTransferMint / fee families
- any unknown future extension (via conservative non-allowlisting)

Currently allowed examples include metadata/group pointers, interest-bearing
UI behavior, scaled UI amount, permanent delegate, and other extensions that
do not change the raw amount delivered by a normal transfer.

## Why this matters

A CPMM formula can be perfectly correct while the route quote is still wrong
if Token-2022 removes a transfer fee between vault and trader or invokes a
transfer hook with additional requirements.

Mint inspection also exposes raw supply and decimals. PumpSwap needs raw base
mint supply to select its dynamic fee tier from market capitalization.

The next Pump milestone is decoding FeeConfig and resolving the fee tier from
the current pool state + base mint supply.
