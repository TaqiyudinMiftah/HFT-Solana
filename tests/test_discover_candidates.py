"""Discovery indexes cannot create routes from unrelated or unsupported pools."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "discovery", Path(__file__).resolve().parents[1] / "scripts/discover-paper-candidates.py"
)
discovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(discovery)


def row(address, other="token", kind="raydium_cpmm", tvl=10000, volume=100):
    return {"address": address, "mints": [discovery.WSOL, other], "kind": kind,
            "tvl": tvl, "volume24h": volume}


class DiscoveryTests(unittest.TestCase):
    def test_ignores_orphans_duplicates_and_nonfinite_metrics(self):
        rows = [row("a"), row("b"), row("a"), row("orphan", "other"),
                row("bad", tvl=float("nan")), row("bad2", volume=float("inf"))]
        self.assertEqual({r["address"] for r in discovery.connected_candidates(rows, 1000, 10)}, {"a", "b"})

    def test_capacity_never_leaves_an_orphan(self):
        rows = [row("a"), row("b"), row("c", "second"), row("d", "second")]
        self.assertEqual(len(discovery.connected_candidates(rows, 1000, 10, limit=3)), 2)

    def test_aggregator_labels_are_only_discovery_hints(self):
        rows = [{"chainId": "solana", "dexId": "orca", "pairAddress": "unsupported"},
                {"chainId": "solana", "dexId": "pumpswap", "pairAddress": "candidate",
                 "baseToken": {"address": "token"}, "quoteToken": {"address": discovery.WSOL},
                 "liquidity": {"usd": 1000}, "volume": {"h24": 20}}]
        values = discovery.normalize_dexscreener(rows)
        self.assertEqual(len(values), 1)
        self.assertEqual(values[0]["kind"], "pump")
        self.assertEqual(values[0]["source"], discovery.DEXSCREENER)


if __name__ == "__main__":
    unittest.main()
