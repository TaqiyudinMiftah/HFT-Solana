# State consistency and dirty-pool scheduling

Live account streams can update the same pool many times while a search pass is
still running. The engine therefore separates:

1. **latest state** — stored atomically in the PoolCell;
2. **work notification** — a bounded queue containing only PoolId.

## Dirty coalescing

The first update flips a per-pool dirty flag and enqueues its PoolId. Further
updates while that flag is set only replace the latest state; they do not add
more queue entries.

The worker clears the flag immediately after popping and only then loads the
latest state. This avoids losing an update around the pop/processing boundary.

A full bounded queue resets the attempted pool's dirty flag so a later update
can retry. The system prefers dropping work over accumulating stale work.

## Quote consistency

A live quote should use immutable Arc snapshots for every leg, validate the
maximum slot skew, calculate the route, then compare each pool's current
StateVersion with the captured version before emitting an opportunity.

If a generation changed during calculation, the result is stale and should be
discarded/recomputed rather than submitted.
