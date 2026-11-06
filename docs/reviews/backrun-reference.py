"""Proposed DEC-23: exact pre-implementation backrun search expectations.

Integer CPMM accounting only; no simulator imports. Each candidate starts
from the same state after the visible victim, not after another search probe.
"""
import json


def swap(pools, venue, asset, amount):
    other = "X" if asset == "Q" else "Q"
    pool = pools[venue]
    output = pool[other] * amount // (pool[asset] + amount)
    pool[asset] += amount
    pool[other] -= output
    return output


def candidate(amount, path, scale=1, price=0, bid=5):
    pools = {name: {"X": 1000 * scale, "Q": 1000 * scale} for name in "AB"}
    victim_out = swap(pools, "A", "Q", 200 * scale)
    first = swap(pools, path[0], "Q", amount)
    output = swap(pools, path[1], "X", first)
    response_units = 100 + 10 + 2 * (10 + 2 * 5) + 10
    payment_units = 115 if bid else 0
    net = output - amount - price * (response_units + payment_units) - bid
    return {
        "input_Q": str(amount), "path": list(path), "victim_output_X": str(victim_out),
        "intermediate_X": str(first), "output_Q": str(output),
        "conditional_net_Q": str(net), "meets_minimum_1": net >= 1,
        "own_units": str(response_units + payment_units),
        "pools_after_success": {name: {asset: str(n) for asset, n in p.items()}
                                for name, p in pools.items()},
    }


grid = [candidate(amount, path) for amount in [25, 50, 100, 150, 200]
        for path in ["AB", "BA"]]
eligible = [item for item in grid if item["meets_minimum_1"]]
best = min(eligible, key=lambda item: (-int(item["conditional_net_Q"]),
                                     int(item["own_units"]), int(item["input_Q"]),
                                     item["path"]))
assert (best["input_Q"], best["path"], best["output_Q"], best["conditional_net_Q"]) == (
    "100", ["B", "A"], "116", "11")
assert [int(item["conditional_net_Q"]) for item in grid if item["path"] == ["B", "A"]] == [3, 9, 11, 6, -6]
assert best["pools_after_success"] == {
    "A": {"X": "924", "Q": "1084"}, "B": {"X": "910", "Q": "1100"}}

priced_small = candidate(100, "BA", price=1)
assert priced_small["conditional_net_Q"] == "-264"
priced_large = candidate(100000, "BA", scale=1000, price=1)
assert (priced_large["victim_output_X"], priced_large["intermediate_X"],
        priced_large["output_Q"], priced_large["conditional_net_Q"]) == (
    "166666", "90909", "118032", "17752")

# In an unrelated builder prefix, public standalone victim is already included.
# Groups sharing it cannot include it twice; there are no canonical searcher fees.
victim_only = {"A": {"X": "834", "Q": "1200"}, "B": {"X": "1000", "Q": "1000"}}

print(json.dumps({
    "status": "pre-implementation expectations for Proposed DEC-23",
    "grid": grid,
    "best": best,
    "public_greedy": {
        "send": "7", "visible": "8", "reaction": "9", "bundle_arrival": "10",
        "cutoff": "10", "target_block": "1", "canonical_receipts": "3",
        "victim_receipts": "1", "searcher_initial_Q": "10000", "searcher_final_Q": "10011",
        "builder_Q": "5", "native_fees_Q": "0", "units": ["120", "160", "115"],
        "total_used": "395", "searcher_nonce": "2", "victim_nonce": "1",
        "search_prefix_attempts": "1", "search_candidates": "10",
    },
    "private_without_searcher": {
        "search_candidates": "0", "submitted_bundles": "0", "canonical_receipts": "1",
        "searcher_final_Q": "10000", "builder_Q": "0", "pools": victim_only,
    },
    "late_bundle": {
        "builder_delay": "2", "arrival": "11", "target_block": "1",
        "canonical_receipts": "1", "searcher_final_Q": "10000", "builder_Q": "0",
        "outcome": "target expired, no standalone response or payment",
    },
    "fifo_same_inputs": {
        "chosen": "standalone victim at 9 before bundle at 10",
        "canonical_receipts": "1", "searcher_final_Q": "10000", "builder_Q": "0",
        "outcome": "group invalidated by already included shared victim",
    },
    "competing_searchers": {
        "bids_Q": ["5", "7"], "best_input_Q_each": "100",
        "conditional_net_Q_each": ["11", "9"], "greedy_winner_bid_Q": "7",
        "winner_Q": "10009", "loser_Q": "10000", "builder_Q": "7",
        "victim_receipts": "1", "canonical_receipts": "3",
    },
    "priced_small": {
        "counterfactual_candidate": priced_small,
        "submitted_bundles": "0", "searcher_canonical_fees_Q": "0",
    },
    "priced_large": {
        "candidate": priced_large, "victim_price": "0", "own_price": "1",
        "response_fee_Q": "160", "payment_fee_Q": "115", "bid_Q": "5",
        "builder_Q": "280", "searcher_initial_Q": "1000000", "searcher_final_Q": "1017752",
        "minimum_guard_Q": "121",
    },
    "declared_limit_boundary": {
        "victim_limit": "500", "response_limit": "500", "payment_limit": "200",
        "minimum_block_limit": "620", "used_if_success": "395",
        "block_limit_619": "response envelope cannot fit after victim; no search bundle",
        "block_limit_620": "all three members fit sequentially",
    },
}, indent=2))
