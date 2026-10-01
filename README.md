# HFT-Solana

Research and implementation workspace for a low-latency Solana arbitrage searcher inspired by on-chain analysis of high-frequency wallets.

## Current scope

V1 is intentionally **paper-trading first**:

- Local pool-state cache
- Precomputed 2-3 leg cycles
- Integer-only CPMM quoting
- Opportunity sizing
- Cashback-aware effective PnL
- Landing-policy model (Direct / Jito / Helius Sender)
- No mainnet transaction submission until quote parity tests pass

## Architecture

```text
Yellowstone / Geyser
        |
        v
 Local Pool State
        |
        v
 Precomputed Cycles
        |
        v
 Marginal Filter
        |
        v
 Exact Local Quote
        |
        v
 Size Optimizer
        |
        v
 Opportunity Queue
        |
        v
 Landing Policy
        |
        v
 Paper Execution / Metrics
```

## Safety rule

The initial implementation must not submit live trades. Production execution is gated behind exact quote-conformance tests against official DEX logic and explicit configuration.

## Development

Development will be staged and reviewed in branches/PRs. The first implementation target is a CPMM-only end-to-end prototype, followed by PumpSwap and Raydium adapters, then Meteora DLMM.
