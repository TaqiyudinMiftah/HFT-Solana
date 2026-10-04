# Paper state reactor

The reactor is the boundary between raw Yellowstone events and decoded DEX
pool state.

```text
FeedEvent
   |
   v
AccountJournal
   |
   v
account -> pool dependency index
   |
   v
recipe rebuild
   |
   +--> PoolUpdated(PoolState)
   |
   +--> PoolInvalidated(reason)
```

## Pool recipes

A Pump recipe depends on Pool, FeeConfig, Pump Global, PumpSwap GlobalConfig,
two vaults and two mints.

A Raydium CPMM recipe depends on PoolState, AmmConfig, two vaults and two
mints.

Shared config accounts may fan out to many pools; a rare global update
therefore rebuilds every dependent quote state.

## Hot-account coherence

The reactor does not compare the last-write slot of every dependency. Static
mint/config accounts can remain valid for many slots without being rewritten.

Instead it checks only accounts that should move together during a swap:

- PumpSwap: base vault + quote vault
- Raydium CPMM: PoolState + vault 0 + vault 1

If their latest slots exceed the configured skew, the current pool is
invalidated until the counterpart updates arrive.

## StateVersion

A successful rebuild emits a local monotonic pool generation. Slot and
write-version come from the hot account set. The local generation ensures a
search calculation can detect any rebuild even when multiple dependency
changes happen within the same slot.

## Fork rollback

When `DiscardBanks` removes the visible raw account version, the journal
restores its retained predecessor. The reactor then rebuilds every dependent
pool from that restored raw state.

No network submission or trading occurs in this layer.
