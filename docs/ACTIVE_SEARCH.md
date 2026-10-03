# Active search snapshots

Cycle search does not read PoolCell repeatedly while probing position sizes.

Instead it captures one immutable Arc snapshot per active leg:

```text
ActivePoolStore
      |
      v
capture_active_cycle
      |
      +--> slot-skew check
      |
      v
immutable CycleSnapshot
      |
      +--> probe q/4
      +--> probe q/2
      +--> probe q
      +--> probe 2q
      +--> probe 4q
      |
      v
generation revalidation
      |
      +--> unchanged -> candidate
      +--> updated/invalidated -> reject
```

This prevents the size optimizer from mixing reserves from different market
states.

An invalidated pool is represented by absence from ActivePoolStore and causes
`SearchError::InactivePool`.

The current optimizer is still the simple five-probe V1. Peak bracketing and
integer refinement remain a later optimization.
