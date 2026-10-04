# Paper runtime runbook

## 1. Prepare config

Copy `config/paper.example.json` and replace placeholder addresses with real
pool, vault, mint, fee/config, and cycle data.

For Meteora DLMM, configure:

- `lb_pair`
- an initial `bin_arrays` window
- optional `bitmap_extension`
- `mint_x`
- `mint_y`

The initial bin list is a bootstrap hint, not a permanent ceiling. Yellowstone
subscribes to all BinArrays owned by each configured LbPair and the reactor
registers newly observed arrays automatically.

## 2. Set market-data endpoints

```bash
export HFT_YELLOWSTONE_ENDPOINT="https://your-yellowstone-endpoint"
export HFT_YELLOWSTONE_X_TOKEN="optional-token"
export HFT_SOLANA_RPC_URL="https://your-solana-rpc"
```

Using the RPC bootstrap is strongly recommended. Without it, readiness depends
on every required account being observed in the live stream after process
start.

## 3. Start

```bash
cargo run --release --features yellowstone --bin paper_yellowstone -- config/paper.json
```

There is no private-key argument and no live transaction path.

## 4. Observe bootstrap

Expected bootstrap diagnostics include:

- explicit account count
- scoped DLMM BinArray count
- RPC context slot
- buffered events applied
- stale/ambiguous buffered events rejected
- initial paper opportunity count

A line containing `PAPER_DLMM_BOOTSTRAP_INCOMPLETE` means the current DLMM
bin window could not yet be made quote-complete. The engine fails closed for
that pool.

## 5. Observe live output

Useful output families:

- `PAPER_OPPORTUNITY`
- `PAPER_LANDING`
- `PAPER_LANDING_SKIP`
- `PAPER_DLMM_REFRESH_REQUIRED`
- final aggregate paper/latency/landing statistics

Landing lines are EV simulations only. They do not submit transactions.

## Coherence rules

For PumpSwap, Raydium CPMM, and DAMM v2, the reactor validates configured hot
account consistency.

For DLMM, bank-scoped updates are held behind Yellowstone
`SLOT_COMPLETED`. The state is invalid while a relevant bank is open and is
published only after the fence. BinArrays not touched by that bank may remain
at earlier slots.

## Bootstrap ordering rule

Yellowstone starts before RPC bootstrap to avoid a blind window. After the RPC
snapshot returns, only a finite already-buffered prefix is replayed. Events
that arrive later remain queued for the normal event loop.

For accounts present in the RPC snapshot:

- older slot -> reject
- same slot -> reject as ordering-ambiguous
- strictly newer slot -> replay

For accounts absent from the snapshot, buffered live updates remain eligible.
