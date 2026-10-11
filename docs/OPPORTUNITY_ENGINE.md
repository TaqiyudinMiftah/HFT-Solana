# Opportunity engine

The opportunity engine consumes dirty pool ids from ActivePoolStore and
evaluates only precomputed cycles that reference that pool.

## Snapshot deduplication

A cycle stores the StateVersion vector from its last evaluation. If pool A and
pool B both become dirty from the same market-state transition, the first dirty
event evaluates the cycle and the second sees the identical version vector and
skips it.

## One snapshot per sizing pass

The flow is:

```text
dirty PoolId
   |
   v
pool_to_cycles
   |
   v
capture immutable active snapshot
   |
   +--> snapshot-version dedupe
   |
   +--> cheap minimum probe
   |
   +--> five-point V1 size optimizer
   |
   +--> minimum effective-profit filter
   |
   +--> generation revalidation
   |
   v
Opportunity
```

The same captured reserves are used for every size probe. A pool update or
invalidation before emission fails final validation.

The engine currently emits gross/effective paper opportunities only. Landing
EV, priority fee and relay-tip selection remain downstream.
