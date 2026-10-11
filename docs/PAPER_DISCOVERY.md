# Discover pools and measure paper rejections

Pool discovery and RPC observations are read-only. There is no signer,
transaction simulation, submission, or realized PnL in this workflow.
Increasing coverage does not guarantee a profitable observation within a minute.

## Discovery boundaries

`scripts/discover-paper-candidates.py` uses Python 3.8+ and only its standard
library. It queries two pages of the public Raydium WSOL index and two Meteora
DAMM v2 index orderings. It then checks at most twelve related token indexes
from DEX Screener to find additional pools, including PumpSwap. There are no
credentials in these index requests, unbounded retries, or program-wide RPC scans.

The default discovery thresholds are index TVL >= $1,000 and 24-hour volume >=
$10. These metrics rank candidates; they do not certify token quality, current
activity, liquidity at every trade size, or quote economics. Addresses, not
symbols, identify tokens. Duplicate pool addresses and unrelated pairs are
excluded. Discovery is bounded to 80 candidates with multiple WSOL pools per pair.

Sources and schemas:

- [Raydium API](https://api-v3.raydium.io/docs/)
- [Meteora DAMM v2 pools API](https://docs.meteora.ag/api-reference/damm-v2/pools/pools)
- [DEX Screener API](https://docs.dexscreener.com/api/reference)
- [Official Pump AMM IDL](https://github.com/pump-fun/pump-public-docs/blob/main/idl/pump_amm.json)
- [Official Raydium PoolState](https://github.com/raydium-io/raydium-cp-swap/blob/master/programs/cp-swap/src/states/pool.rs)

`paper_discover` verifies candidates using `HFT_SOLANA_RPC_URL` and existing Rust
decoders. A DEX Screener label cannot distinguish Raydium CPMM from AMM v4/CLMM,
or DAMM v2 from DLMM: unsupported owners/layouts are rejected. Mint pairs must
match on-chain state. Raydium swap status/open time and Meteora activation are
checked. Pump dependencies use PDA seeds from the official IDL.

It selects up to fifteen connected pools by activity, checks all dependency
accounts in one RPC context, rejects unsafe or non-legacy mints, and requires
every pool to become quote-ready in the existing pipeline. Rejected pools are
removed and selection is retried from the finite candidate list. It creates
both directed two-leg cycles for each distinct compatible pair of pools. Four
pools for one pair yield twelve cycles. WSOL is always the start asset, so landing
costs and effective profit use lamports. Search and landing settings are inherited
from the supplied template; discovery does not lower profitability thresholds.

Config and report outputs must be new paths. Existing files are never overwritten.
New local configs match `/config/paper.*.local.json` in `.gitignore`.

## Native Windows reproduction

First use the Rust/MSVC/OpenSSL setup in [LOCAL_SETUP.md](LOCAL_SETUP.md).
On a memory-constrained PC, use one Cargo job:

```powershell
$env:CARGO_BUILD_JOBS = '1'
cargo fmt --all -- --check
cargo test --locked --all-targets --all-features
python -m unittest discover -s tests -p 'test_discover_candidates.py' -v
cargo build --release --locked --features yellowstone --bin paper_yellowstone --examples
```

The following commands preserve prior discoveries, configs, and reports:

```powershell
$runId = Get-Date -Format 'yyyyMMdd-HHmmss'
New-Item -ItemType Directory -Path 'target\local-setup' -Force | Out-Null
$candidatesPath = "target\local-setup\paper-candidates-$runId.json"
$configPath = "config\paper.expanded-$runId.local.json"
$reportPath = "target\local-setup\paper-report-$runId.json"

python .\scripts\discover-paper-candidates.py $candidatesPath
if ($LASTEXITCODE -ne 0) { throw 'Public pool discovery failed' }
. .\scripts\Import-PaperProvider.ps1
.\target\release\examples\paper_discover.exe $candidatesPath config\paper.local.json $configPath 15
if ($LASTEXITCODE -ne 0) { throw 'On-chain pool validation failed' }
.\target\release\examples\paper_bootstrap.exe $configPath --samples 120 --interval-secs 5 --diagnostic-sizes 100000,1000000,10000000 --report $reportPath
```

The three diagnostic inputs are 0.0001, 0.001, and 0.01 WSOL respectively. They
must fall within every cycle's search bounds. Adjust them if the template has
different bounds; all sizes are raw integers. The default template path above
refers to the verified local WSOL config, not the unfilled example placeholders.
On a fresh checkout, `paper_discover` may instead use `config/paper.example.json`
as its template: generated tokens/pools/cycles replace those placeholders.
Its inherited search bounds differ, so choose diagnostic sizes within those
bounds (for the current example: 1000000,10000000,1000000000).

## Bash / WSL / macOS reproduction

After exporting the provider environment locally, use:

```bash
cargo build --release --locked --features yellowstone --bin paper_yellowstone --examples
run_id=$(date +%Y%m%d-%H%M%S)
mkdir -p target/local-setup
candidates_path="target/local-setup/paper-candidates-$run_id.json"
config_path="config/paper.expanded-$run_id.local.json"
report_path="target/local-setup/paper-report-$run_id.json"
python3 scripts/discover-paper-candidates.py "$candidates_path"
target/release/examples/paper_discover "$candidates_path" config/paper.local.json "$config_path" 15
target/release/examples/paper_bootstrap "$config_path" --samples 120 --interval-secs 5 --diagnostic-sizes 100000,1000000,10000000 --report "$report_path"
```

Stop after a failed command. The Windows DPAPI credential file cannot be imported
on another OS; set `HFT_SOLANA_RPC_URL` through that environment's local secret
handling. This Bash sequence is documented for reproduction; local runtime
verification was performed on native Windows.

## Interpret telemetry

`PAPER_SEARCH_STATS` reports actual decisions from the opportunity engine:

| Field | Meaning |
| --- | --- |
| `evaluated` | Distinct cycle state vectors evaluated inside this pipeline |
| `snapshot_unavailable` | Pool unavailable or coherence check failed before quote |
| `duplicate_snapshot` | Same state vector already evaluated through another dirty pool |
| `probe_quote_failed` | Minimum probe could not be quoted |
| `nonpositive_probe` | Probe effective profit <= 0; sizing was not attempted |
| `sizing_failed` | Optimizer did not produce a valid quote |
| `below_minimum_profit` | Optimized effective profit below configured minimum |
| `stale_snapshot` | State changed before final validation |
| `opportunities` | Opportunity passed search filters |

The five quote/sizing rejection fields and `opportunities` partition `evaluated`.
Unavailable and duplicate snapshots are separate counts.

`PAPER_DIAGNOSTIC` quotes the requested additional sizes on the same immutable
snapshot, even when the engine rejected the probe. It cannot emit opportunities
or change filters. `PAPER_PROBE_DISAGREEMENT` flags a nonpositive probe with a
positive larger grid quote; investigate that state/fee/rounding case before
changing the marginal filter. Absence of this flag does not prove that a sparse
grid or the bounded optimizer found the global best size.

`PAPER_RPC_METRICS` reports RPC time, bootstrap/search time, diagnostic time, and
the number of account payloads that changed from the previous accepted sample.
Slot advances alone do not count as account changes. The first sample is a
baseline. Local quote timing includes diagnostic logging; it is not an isolated
hot-loop benchmark or Yellowstone feed-lag measurement.

`PAPER_RUN_SUMMARY` and the optional JSON report aggregate search decisions,
landing selections/rejections, quote failures, unchanged account samples, and
profit ranges per cycle/input. Report `curves` describe DEX-fee-aware effective
profit before execution fees; DEX fees are already reflected in output.
Landing costs are applied separately. Landing probabilities remain demonstration
parameters without a labeled calibration dataset.

## Limits and troubleshooting

- RPC observations require <=100 explicit accounts and no DLMM discovery.
  The final quote state comes from one complete `getMultipleAccounts` response.
- Each new context creates an independent pipeline. Repeated positive readings
  are observations, not unique executable opportunities. RPC does not provide
  write versions, bank ancestry, stream ordering, or proof of future landing.
- The interval is a delay after each completed sample, so 120 samples at five
  seconds take roughly ten minutes plus request/compute time.
- A provider timeout or incomplete snapshot stops the run. A quote error in one
  ready cycle is counted without manufacturing a positive quote. A pool that
  cannot become ready stops the observation.
- HTTP 403 from a public index is distinct from RPC authentication. The discovery
  script supplies an explicit user agent; do not bypass access controls or
  disable TLS verification. HTTP 429 requires lower request volume or provider
  quota; do not run concurrent discovery loops.
- `memory allocation ... failed` is a compiler resource failure. Check available
  RAM, Windows commit capacity, and drive space; close unused applications and
  use `CARGO_BUILD_JOBS=1`. A package-scoped `cargo clean -p hft-solana --profile dev`
  regenerates application artifacts while retaining dependency builds and local
  data. Avoid a full clean when disk space cannot accommodate dependency rebuilds.
- Yellowstone still requires an activated compatible gRPC provider. An Alchemy
  RPC key does not establish streaming access. Use the bounded `yellowstone_probe`
  once access is available, then verify actual account ordering, bootstrap,
  reconnect, and slot fences. No provider upgrade is performed by this workflow.

## Local observation record

The expanded universe validated on 11 October 2026 contained 10 pools, five
legacy SPL mints, sixteen WSOL-start cycles, and 41 dependency accounts at
context slot `455545886`. Pools covered PumpSwap, Raydium CPMM, and Meteora DAMM v2.
The search bounds remained 100,000..10,000,000 lamports and the minimum effective
profit remained 1,000 lamports. The original two-pool config was preserved.

Longer observation results and final verification are recorded in
[LOCAL_SETUP.md](LOCAL_SETUP.md). Local reports, configs, and provider credentials
are excluded from Git.
