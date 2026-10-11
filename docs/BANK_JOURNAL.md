# Bank-aware raw account journal

Processed Yellowstone updates can arrive from banks that are later discarded
during reconnect/fork recovery. Applying them directly to the quote cache
without history can leave state from a dead fork visible.

The raw-account journal stores a small bounded history per subscribed account:

```text
Yellowstone account update
        |
        v
AccountJournal
        |
        +--> current raw account
        |
        +--> prior bank-scoped versions
```

When Yellowstone emits `DiscardBanks`, the journal removes only entries whose
`(generation, slot, bank_id)` exactly match those banks. If the visible state
changes, the affected pubkey is returned so dependent pools can be rebuilt.

If no retained fallback exists, the journal reports `missing_fallback`. The
reactor must re-bootstrap that account instead of pretending the previous pool
state is still valid.

Startup snapshots have no bank id and are retained as fallback.

This layer stores raw account bytes only. It does not decode DEX state, sign
transactions, or submit trades.
