# HFT-Solana

Low-latency Solana arbitrage research and paper-trading engine derived from
on-chain reverse engineering and official DEX math.

## Current scope

The engine is intentionally **paper-mode only**. It does not contain a live
transaction signer/sender.

Implemented paper-path coverage:

- PumpSwap
- Raydium CPMM
- Meteora DAMM v2
- Meteora DLMM
- precomputed 2-3 leg cycles
- integer / official-SDK local quoting
- opportunity sizing
- cashback-aware effective PnL
- landing-policy modelling for Direct / Jito / Helius Sender
- Yellowstone processed account stream
- slot-complete coherence fences
- RPC startup bootstrap + buffered catch-up
- DLMM bin-window fail-closed validation and self-healing discovery
- paper latency / landing EV telemetry

## Runtime architecture

```text
Yellowstone subscribe
        |
        +--> buffer live processed events
        |
RPC bootstrap snapshot
        |
        +--> explicit accounts
        +--> all scoped DLMM BinArrays
        |
        v
deterministic bootstrap catch-up
        |
        v
AccountJournal / slot fences
        |
        v
quote-ready PoolState snapshots
        |
        v
ActivePoolStore
        |
        v
Precomputed Cycles
        |
        v
Exact Local Quote + Size Optimizer
        |
        v
Opportunity
        |
        v
Landing Policy
        |
        v
Paper telemetry only
```

## Running paper Yellowstone mode

Build and run:

```bash
cargo run --release --features yellowstone --bin paper_yellowstone -- config/paper.json
```

Required:

```bash
export HFT_YELLOWSTONE_ENDPOINT="https://..."
```

Optional Yellowstone authentication:

```bash
export HFT_YELLOWSTONE_X_TOKEN="..."
```

Strongly recommended startup bootstrap:

```bash
export HFT_SOLANA_RPC_URL="https://..."
# Optional; defaults to 30 seconds.
export HFT_BOOTSTRAP_TIMEOUT_SECS="30"
```

When `HFT_SOLANA_RPC_URL` is present, the process:

1. starts the Yellowstone subscription first;
2. buffers live processed events;
3. fetches an RPC snapshot for configured accounts;
4. fetches all DLMM BinArrays belonging to configured LbPairs;
5. seeds the reactor without running search on partial state;
6. replays only buffered events that are provably newer than the RPC snapshot;
7. evaluates the fully caught-up state once;
8. continues with the live feed.

Same-slot account updates are deliberately rejected during bootstrap catch-up
because JSON-RPC does not expose account `write_version`; their ordering
relative to the snapshot is ambiguous.

## DLMM coherence

DLMM is not treated like a CPMM with a fixed set of hot accounts.

The runtime uses:

- immutable Yellowstone owner + memcmp scopes to receive all BinArrays for
  configured LbPairs;
- slot-complete fences before publishing bank-scoped DLMM snapshots;
- fail-closed validation when the currently required bin window is incomplete;
- automatic registration of newly observed BinArrays for known LbPairs.

Unchanged BinArrays are allowed to remain from older slots. The engine does
not incorrectly require every array in a DLMM pool to be rewritten together.

## CI

CI requires both:

```bash
cargo fmt --all -- --check
cargo test --all-targets --all-features
```

Official Meteora SDK repositories are pinned to commits so quote parity cannot
change silently.

## Safety rule

Live transaction submission is intentionally absent. Production execution
remains gated behind explicit implementation, review, and separate execution
tests. Paper landing-policy output must not be confused with transaction
submission.
