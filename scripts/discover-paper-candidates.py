#!/usr/bin/env python3
"""Read public DEX indexes; produce candidates, never authoritative quotes."""
import argparse
import collections
import datetime
import json
import math
from pathlib import Path
import time
import urllib.error
import urllib.parse
import urllib.request

WSOL = "So11111111111111111111111111111111111111112"
CPMM = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C"
RAYDIUM = "https://api-v3.raydium.io"
METEORA = "https://damm-v2.datapi.meteora.ag"
DEXSCREENER = "https://api.dexscreener.com"


def fetch(base, path, params):
    request = urllib.request.Request(
        base + path + "?" + urllib.parse.urlencode(params),
        headers={"User-Agent": "HFT-Solana-paper-research/1.0"},
    )
    # No credentials, transactions, unbounded retries, or getProgramAccounts.
    with urllib.request.urlopen(request, timeout=30) as response:
        value = json.load(response)
    time.sleep(0.3)
    return value


def normalize_raydium(value):
    if value.get("success") is not True:
        raise ValueError("Raydium index did not report success")
    result = []
    for row in value["data"]["data"]:
        if row.get("programId") != CPMM:
            continue
        result.append({
            "address": row["id"], "kind": "raydium_cpmm",
            "mints": [row["mintA"]["address"], row["mintB"]["address"]],
            "tvl": row["tvl"], "volume24h": row["day"]["volume"],
            "source": RAYDIUM,
        })
    return result


def normalize_meteora(value):
    result = []
    for row in value["data"]:
        if row.get("is_blacklisted") is not False:
            continue
        result.append({
            "address": row["address"], "kind": "meteora_damm",
            "mints": [row["token_x"]["address"], row["token_y"]["address"]],
            "tvl": row["tvl"], "volume24h": row["volume"]["24h"],
            "source": METEORA,
        })
    return result


def normalize_dexscreener(rows):
    kinds = {"raydium": "raydium_cpmm", "meteora": "meteora_damm", "pumpswap": "pump"}
    result = []
    for row in rows:
        if row.get("chainId") != "solana" or row.get("dexId") not in kinds:
            continue
        result.append({
            "address": row["pairAddress"], "kind": kinds[row["dexId"]],
            "mints": [row["baseToken"]["address"], row["quoteToken"]["address"]],
            "tvl": (row.get("liquidity") or {}).get("usd", 0),
            "volume24h": (row.get("volume") or {}).get("h24", 0),
            "source": DEXSCREENER,
        })
    # Labels cannot distinguish all Raydium/Meteora programs. Native owner
    # checks must reject AMM v4, CLMM and DLMM here; never quote these as CPMM.
    return result


def connected_candidates(rows, minimum_tvl, minimum_volume, limit=80):
    groups = collections.defaultdict(dict)
    for row in rows:
        pair = tuple(sorted(row["mints"]))
        metrics = (row["tvl"], row["volume24h"])
        if (len(set(pair)) != 2 or WSOL not in pair or
                any(not isinstance(v, (int, float)) or not math.isfinite(v) for v in metrics) or
                row["tvl"] < minimum_tvl or row["volume24h"] < minimum_volume):
            continue
        groups[pair].setdefault(row["address"], row)
    groups = [list(group.values()) for group in groups.values() if len(group) >= 2]
    groups.sort(key=lambda group: (-sum(row["volume24h"] for row in group),
                                   tuple(sorted(group[0]["mints"]))))
    result = []
    for group in groups:
        group.sort(key=lambda row: (-row["volume24h"], -row["tvl"], row["address"]))
        # Leave room for owner validation to reject unsupported pool labels.
        group = group[:8]
        if len(result) + len(group) <= limit:
            result.extend(group)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--minimum-tvl", type=float, default=1000)
    parser.add_argument("--minimum-volume24h", type=float, default=10)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("output exists; choose a new path to preserve the previous discovery")
    if any(not math.isfinite(v) or v < 0 for v in (args.minimum_tvl, args.minimum_volume24h)):
        parser.error("liquidity and activity limits must be finite and nonnegative")
    rows = []
    for page in (1, 2):
        value = fetch(RAYDIUM, "/pools/info/mint", {
            "mint1": WSOL, "poolType": "standard", "poolSortField": "volume24h",
            "sortType": "desc", "pageSize": 100, "page": page,
        })
        rows.extend(normalize_raydium(value))
    for order in ("volume_24h:desc", "tvl:desc"):
        value = fetch(METEORA, "/pools", {"query": "SOL", "sort_by": order, "page_size": 1000})
        rows.extend(normalize_meteora(value))
    connected = connected_candidates(rows, args.minimum_tvl, args.minimum_volume24h)
    seeds = list(dict.fromkeys(mint for row in connected for mint in row["mints"] if mint != WSOL))
    for row in rows:
        if row["kind"] == "raydium_cpmm" and WSOL in row["mints"]:
            for mint in row["mints"]:
                if mint != WSOL and mint not in seeds:
                    seeds.append(mint)
    for mint in seeds[:12]:
        rows.extend(normalize_dexscreener(fetch(DEXSCREENER, "/token-pairs/v1/solana/" + mint, {})))
    candidates = connected_candidates(rows, args.minimum_tvl, args.minimum_volume24h)
    if not candidates:
        raise ValueError("no connected pool groups met the discovery limits")
    result = {
        "discovered_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "minimum_tvl": args.minimum_tvl, "minimum_volume24h": args.minimum_volume24h,
        "note": "Index metrics are hints. Run paper_discover for on-chain validation.",
        "candidates": candidates,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    print("PAPER_DISCOVERY_CANDIDATES count=" + str(len(candidates)) +
          " pairs=" + str(len({tuple(sorted(row['mints'])) for row in candidates})))


if __name__ == "__main__":
    try:
        main()
    except urllib.error.HTTPError as error:
        raise SystemExit("Public DEX index request failed: HTTP " + str(error.code))
    except (urllib.error.URLError, TimeoutError, ValueError, KeyError, TypeError):
        raise SystemExit("Public DEX index request or schema validation failed; no config written")
