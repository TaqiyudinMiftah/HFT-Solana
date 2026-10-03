# Active pool store

The active store is the final state gate before cycle search.

Only a reactor `PoolUpdated` output creates or replaces an active PoolCell.
A `PoolInvalidated` output removes that PoolCell immediately.

Both transitions mark the pool dirty so every precomputed cycle containing the
pool is revisited:

- update -> search against the new state
- invalidation -> cycle sees the missing pool and cannot emit an opportunity

Repeated changes to the same pool are coalesced by `DirtyPoolQueue`; the
latest ArcSwap state remains available when the search worker pops the PoolId.

This means partially updated vault pairs and discarded-fork state are removed
from the search universe rather than being traded optimistically.
