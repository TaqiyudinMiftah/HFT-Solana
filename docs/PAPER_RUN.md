# Running Yellowstone paper mode

The repository now has a runnable paper-trading binary. It subscribes only to
explicit accounts referenced by the configured pool recipes, rebuilds
fork-aware pool state, evaluates preconfigured cycles, and prints opportunities.

It does **not** contain a signer or live transaction sender.

## Check RPC bootstrap without Yellowstone

After replacing placeholders with verified account addresses, one read-only
bootstrap and local quote pass can run with only `HFT_SOLANA_RPC_URL`:

```bash
cargo run --release --locked --features yellowstone --example paper_bootstrap -- config/paper.local.json
```

The `yellowstone` feature enables the existing RPC/decoder dependencies; this
example does not open a gRPC subscription. By default it exits after checking each pool
is active and quoting each cycle at `minimum_probe`. It reuses the runtime's
config loader, bootstrap reconciliation, and integer quote math. Fee/time
models use the local clock, as in the paper runtime. Output is a snapshot
prediction, not transaction simulation or realized PnL. Errors from the RPC
provider are suppressed because they may contain a credential-bearing URL.

If streaming is unavailable, observe a bounded sequence of independent RPC
snapshots with the same example:

```bash
cargo run --release --locked --features yellowstone --example paper_bootstrap -- config/paper.local.json --samples 60 --interval-secs 5
```

Each accepted sample builds a fresh pipeline from a single `getMultipleAccounts`
response. The example supports at most 100 explicit accounts and rejects DLMM
discovery, which requires additional RPC responses. Mixed contexts, incomplete
snapshots, and non-startup events are rejected; duplicate or older context slots
are skipped. It waits after each completed request and never sends a catch-up
burst. RPC errors stop the run; there is no unbounded retry loop in the example.

`PAPER_RPC_OPPORTUNITY` and `PAPER_RPC_LANDING` label per-snapshot predictions.
Observations are not deduplicated across newer snapshots, and RPC does not expose
account write versions or bank ancestry. This mode samples state rather than
measuring Yellowstone feed latency or reconstructing processed forks. Provider
RPC quotas still apply. Landing probabilities remain demonstration parameters
unless calibrated from labeled outcomes. No transaction is submitted.
When landing is configured, all cycles must start in WSOL so lamport costs and
raw profit use the same units; otherwise remove `landing` for quote observations.

To expand the pool universe, inspect rejection reasons, and quote additional
sizes without changing the search filter, follow [PAPER_DISCOVERY.md](PAPER_DISCOVERY.md).
For example:

```bash
cargo run --release --locked --features yellowstone --example paper_bootstrap -- config/paper.expanded.local.json --samples 120 --interval-secs 5 --diagnostic-sizes 100000,1000000,10000000 --report target/local-setup/new-paper-report.json
```

`PAPER_SEARCH_STATS` counts actual engine decisions. Additional-size diagnostics
remain separate from emitted opportunities; a positive diagnostic does not
bypass the probe filter. Output/report paths must be new files.

## Diagnose Yellowstone access

After loading the provider environment, check the subscription method with a
bounded, slot-only Rust probe:

```bash
cargo run --release --locked --features yellowstone --example yellowstone_probe
```

The probe checks TLS, `Subscribe`, and a processed slot carrying `bank_id`;
it does not verify account ordering or replay/reconnect compatibility. Connection,
subscription, and the first processed-slot wait each have a 10-second timeout.
Server messages and credential URLs are omitted from errors.

`Unimplemented` means the method is unavailable through the tested endpoint/access.
Verify streaming activation and the gRPC URL with the provider; changing DEX math
or using a JSON-RPC URL will not resolve it. `Unauthenticated` concerns credentials,
`PermissionDenied` concerns access, and `ResourceExhausted` concerns quota.
The production feed limits connection and initial subscription to 15 seconds each;
HTTPS endpoints explicitly enable TLS with native certificate roots; plaintext
HTTP remains available for local test servers. Certificate verification stays enabled.
The established stream has no artificial lifetime limit. A startup failure preserves
the feed error rather than racing it with a generic readiness error.

For Windows environment and credential loading, see [LOCAL_SETUP.md](LOCAL_SETUP.md).

## Bank delivery fence

The adapter releases a DLMM snapshot only on the matching bank-scoped
`SLOT_PROCESSED` marker, after account notifications. `SLOT_COMPLETED` is a
shred-reception status and does not identify a bank. Updates without a bank id
cannot release this fence. Providers must preserve this account/processed
ordering and support the bank metadata/recovery used by the client.

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

The top-level `tokens` array maps each local token ID to a real mint address:
index 0 is token 0, index 1 is token 1, and so on. Cycle edges are validated
against the configured pool mint pair and direction before Yellowstone is
connected. Every cycle must:

1. start at `start_token`;
2. be token-contiguous edge-to-edge;
3. contain two or three edges;
4. use a direction whose input/output mints match the referenced pool;
5. return to `start_token`.

The config loader rejects invalid topology or mint-direction mismatches before
connecting to Yellowstone.

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
