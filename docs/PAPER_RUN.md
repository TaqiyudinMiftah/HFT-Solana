# Running Yellowstone paper mode

The repository now has a runnable paper-trading binary. It subscribes only to
explicit accounts referenced by the configured pool recipes, rebuilds
fork-aware pool state, evaluates preconfigured cycles, and prints opportunities.

It does **not** contain a signer or live transaction sender.

## Build

```bash
cargo test --all-targets --all-features
cargo build --release --features yellowstone --bin paper_yellowstone
```

## Environment

```bash
export HFT_YELLOWSTONE_ENDPOINT="https://your-geyser-endpoint"
export HFT_YELLOWSTONE_X_TOKEN="your-token"   # optional if provider needs none
```

Do not commit endpoint credentials or tokens.

## Run

```bash
cargo run --release --features yellowstone --bin paper_yellowstone -- config/paper.example.json
```

The example file is a template. Replace every `REPLACE_WITH_...` value with a
real Solana account address before running.

## Pool ordering and cycle IDs

Pools receive dense numeric IDs from their order in the JSON `pools` array:

- first pool = 0
- second pool = 1
- etc.

Cycle edges reference those IDs.

Token IDs are local integers chosen by the config author. They do not need to
match any on-chain identifier, but they must be consistent across edges. Every
cycle must:

1. start at `start_token`;
2. be token-contiguous edge-to-edge;
3. contain two or three edges;
4. return to `start_token`.

The config loader rejects invalid topology before connecting to Yellowstone.

## Search values

All token amounts are raw integer units.

`initial_seed`
: Initial size hint used by the V1 optimizer.

`minimum_probe`
: Small quote used to reject obviously unprofitable cycles.

`max_size`
: Maximum raw input amount considered by the V1 optimizer.

`minimum_effective_profit`
: Minimum raw-unit effective profit after supported cashback accounting, but
  before landing costs.

`max_slot_skew`
: Maximum slot difference allowed across the pools captured for one cycle.

`expected_cu`
: Initial compute-unit estimate retained on emitted paper opportunities.

## Output

Each opportunity is printed as one line:

```text
PAPER_OPPORTUNITY cycle=0 amount_in=... expected_out=... effective_profit=... expected_cu=... created_ns=...
```

If the opportunity output channel cannot keep up, the paper runtime drops
paper opportunities rather than backpressuring market-data ingestion. Dropped
opportunities are counted and reported on shutdown.

## Current limitation

Pool discovery is intentionally static in this milestone: the JSON file lists
the exact PumpSwap/Raydium accounts to subscribe to. Automatic discovery of new
pools and graph hot-reload comes after the live paper pipeline is proven stable.
