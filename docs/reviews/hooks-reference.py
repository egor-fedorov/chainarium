"""Proposed DEC-25 expectations, independent integer arithmetic before implementation.

CPMM (1000 X, 1000 Q), fee retained in pool. BeforeSwap: input fee,
max input, counter. AfterSwap: counter, pay 3 Q from the hook's own reserve.
Each hook entry and typed instruction costs 10; actual ledger transfer costs 5.
No imports from Chainarium.
"""
import json


def swap(amount, fee_bps):
    effective = amount * (10000 - fee_bps)
    return 1000 * effective // (1000 * 10000 + effective)


swap_units = 100 + 10 + (10 + 3 * 10) + 2 * 5 + (10 + 2 * 10 + 5)
lp_units = 100 + 10 + (10 + 10) + 2 * 5 + (10 + 2 * 10 + 5)
assert swap_units == 195
assert lp_units == 175
small = swap(50, 0)
large = swap(100, 1000)
assert (small, large) == (47, 82)


def success(amount, output):
    return {
        "input_X": str(amount), "output_Q": str(output), "reward_Q": "3",
        "used": str(swap_units), "native_fee_Q": str(swap_units),
        "sender_X": str(1000-amount), "sender_Q": str(1000+output+3-swap_units),
        "pool_X": str(1000+amount), "pool_Q": str(1000-output),
        "hook_Q": "17", "counter": "2", "sender_nonce": "1",
    }


def reverted(units, reason, hook_q=20):
    return {"reason": reason, "used": str(units), "native_fee_Q": str(units),
            "sender_X": "1000", "sender_Q": str(1000-units),
            "pool_X": "1000", "pool_Q": "1000", "hook_Q": str(hook_q),
            "counter": "0", "sender_nonce": "1", "committed_hook_effects": []}


print(json.dumps({
    "status": "pre-implementation expectations for Proposed DEC-25",
    "initial": {"pool_X": "1000", "pool_Q": "1000", "sender_X": "1000",
                "sender_Q": "1000", "hook_Q": "20", "builder_Q": "0",
                "base_fee_bps": "0", "native_price": "1", "native_limit": "500",
                "counter": "0", "sender_nonce": "0"},
    "fee_rule_X": {"threshold": "100", "below_bps": "0", "at_or_above_bps": "1000",
                   "max_input": "200"},
    "small_swap": success(50, small),
    "large_swap": success(100, large),
    "unhooked_control": {"input_X": "100", "output_Q": str(swap(100,0)),
                         "used": "120", "sender_Q": str(1000+swap(100,0)-120),
                         "sender_X": "900", "hook_Q": "20"},
    "late_assertion": reverted(swap_units+10, "AssertBalance"),
    "input_201": reverted(100+10+10+10+10, "HookInputLimit"),
    "minimum_83": reverted(100+10+10+3*10, "min_out"),
    "hook_cash_2": reverted(swap_units, "insufficient hook balance", 2),
    "exhausted_at_194": reverted(swap_units-1, "ResourceExhausted"),
    "lp_round_trip": {
        "initial_total_shares": "1000", "initial_sender_shares": "0",
        "after_add": {"input_X": "100", "input_Q": "100", "minted_shares": "100",
                      "used": str(lp_units), "sender_X": "900", "sender_Q": str(1000-100+3-lp_units),
                      "pool_X": "1100", "pool_Q": "1100", "total_shares": "1100",
                      "hook_Q": "17", "counter": "2", "sender_nonce": "1"},
        "after_remove": {"output_X": "100", "output_Q": "100", "burned_shares": "100",
                         "used": str(lp_units), "sender_X": "1000", "sender_Q": str(1000+6-2*lp_units),
                         "pool_X": "1000", "pool_Q": "1000", "total_shares": "1000",
                         "hook_Q": "14", "counter": "4", "builder_Q": str(2*lp_units), "sender_nonce": "2"},
        "late_add_failure": reverted(lp_units+10, "AssertBalance"),
    },
    "boundaries": {"fee_0_and_9999": "accepted", "fee_10000": "invalid config",
                   "instructions_16": "accepted", "instructions_17": "invalid config",
                   "bindings_64": "accepted", "bindings_65": "invalid config",
                   "counter_MAX_increment": "ProtocolRevert HookStateExhausted, no wrap",
                   "capability_violation": "invalid config; runtime defense rejects without applying effect"},
}, indent=2))
