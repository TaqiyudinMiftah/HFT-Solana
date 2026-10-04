# Architecture

## Goal

Build a low-latency Solana arbitrage research engine that can reproduce DEX
quotes locally, detect 2-3 leg cycles, optimize size, and estimate landing EV.

The project starts in **paper mode**. Live transaction submission remains
disabled until local quotes have exact integer parity with authoritative DEX
implementations across a large fixture set.

## Hot path

```text
account update
    |
    v
single-writer state shard
    |
    v
mark pool dirty / collapse repeated updates
    |
    v
precomputed pool -> cycle index
    |
    v
marginal filter
    |
    v
exact local quote
    |
    v
size optimizer
    |
    v
cashback-aware effective PnL
    |
    v
execution-cost / landing policy
    |
    v
paper opportunity record
```

## Design principles

1. No HTTP quote API in the opportunity hot path.
2. Integer arithmetic only for DEX economics.
3. Precompute cycle topology; update only affected cycles.
4. Prefer single-writer state ownership over a giant globally locked map.
5. Use bounded queues; stale work is worse than dropped work.
6. Revalidate state generation before emitting/submitting an opportunity.
7. Keep DEX-specific quote semantics isolated behind adapters.
8. Treat Jito tip, Helius/SWQoS tip, and Solana priority fee as separate bids.

## Planned stages

### Stage 1 — CPMM research core
- Generic integer CPMM primitive
- Cycle representation
- Local route quote
- Seed-based sizing
- Paper opportunity output

### Stage 2 — Exact DEX parity
- PumpSwap account decoder + exact fee/cashback logic
- Raydium CPMM account decoder + exact fee logic
- Fixture-based conformance tests

### Stage 3 — Live state
- Yellowstone/Geyser ingest
- Single-writer state shards
- Dirty-pool coalescing
- Precomputed cycle index

### Stage 4 — Meteora
- DAMM v2 exact adapter
- DLMM bin-state cache
- Bin-aware quote and size optimizer

### Stage 5 — Landing research
- Priority-fee estimator
- Jito and Helius policy models
- Landing probability/EV telemetry
- Still paper mode by default

### Stage 6 — Atomic execution
- Separate on-chain executor program
- Profit guard
- Opportunity deduplication
- Explicit operator-controlled live-mode gate
