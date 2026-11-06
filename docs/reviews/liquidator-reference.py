"""Proposed DEC-24: integer expectations before implementing liquidator policy.

Uses the accepted liquidation rounding and one fee-free CPMM sale. No imports
from the simulator; native wrapper costs are proposed explicitly in DEC-24.
"""
import json


def trial(request, scale=1, price=0, flash=False):
    debt, collateral = 80 * scale, 6 * scale
    cap = (collateral * 11 * 10000 + 11000 - 1) // 11000
    paid = min(request, debt, cap)
    seized = min(collateral, paid * 11000 // (11 * 10000))
    output = 1500 * scale * seized // (100 * scale + seized)
    flash_fee = (request * 5 + 9999) // 10000 if flash else 0
    units = 190 if flash else 170
    loss = debt - paid if seized == collateral else 0
    return dict(request_Q=str(request), paid_Q=str(paid), seized_X=str(seized),
                sale_Q=str(output), flash_fee_Q=str(flash_fee), units=str(units),
                native_fee_Q=str(units * price), net_Q=str(output - paid - flash_fee - units * price),
                cash_Q=str(20 * scale + paid), debt_Q=str(debt - paid - loss),
                claims_Q=str(100 * scale - loss), equity_Q="0", writeoff_Q=str(loss))


small_own = [trial(n) for n in (20, 40, 60, 80)]
small_flash = [trial(n, flash=True) for n in (20, 40, 60, 80)]
large_own = [trial(n, scale=1000, price=1) for n in (20000, 40000, 60000, 80000)]
large_flash = [trial(n, scale=1000, price=1, flash=True) for n in (20000, 40000, 60000, 80000)]
assert [x["net_Q"] for x in small_own] == ["9", "17", "24", "24"]
assert [x["net_Q"] for x in small_flash] == ["8", "16", "23", "23"]
assert [x["net_Q"] for x in large_own] == ["9241", "17522", "24735", "24735"]
assert [x["net_Q"] for x in large_flash] == ["9211", "17482", "24685", "24675"]
assert large_own[2]["sale_Q"] == "84905"

print(json.dumps({
    "status": "pre-implementation expectations for Proposed DEC-24",
    "initial": {"cash_Q": "20", "debt_Q": "80", "claims_Q": "100", "equity_Q": "0",
                "collateral_X": "6", "oracle_Q_per_X": "11", "liquidation_threshold_bps": "8000",
                "bonus_bps": "1000", "close_bps": "10000", "rate": "0",
                "sale_pool_X": "100", "sale_pool_Q": "1500", "sale_fee_bps": "0"},
    "small_own": small_own, "small_flash": small_flash,
    "large_own_price1": large_own, "large_flash_price1": large_flash,
    "selected_request": {"small_Q": "60", "large_Q": "60000", "tie": "smaller requested repayment"},
    "competition_large": {
        "initial_Q_each": "100000", "requests_Q": ["20000", "40000", "60000", "80000"],
        "alice_price": "1", "carol_price": "2", "alice_arrival": "20", "carol_arrival": "21",
        "cutoff": "30", "success_units": "170", "no_debt_revert_units": "120",
        "fifo": {"winner": "alice", "alice_Q": "124735", "carol_Q": "99760", "builder_Q": "410"},
        "fee_priority": {"winner": "carol", "alice_Q": "99880", "carol_Q": "124565", "builder_Q": "460"},
        "shared_result": {"cash_Q": "80000", "debt_Q": "0", "claims_Q": "80000", "writeoff_Q": "20000",
                          "seized_X": "6000", "sale_Q": "84905", "nonce_each": "1"},
    },
    "flash_large": {"principal_Q": "60000", "repayment_Q": "60030", "fee_Q": "30",
                    "initial_Q": "1000", "final_Q": "25685", "builder_Q": "190", "net_Q": "24685"},
    "actual_collateral_after_prior_partial_liquidation": {
        "prior_paid_Q": "20", "prior_seized_X": "2", "current_debt_Q": "60", "current_collateral_X": "4",
        "own_existing_X": "9", "request_Q": "60", "actual_paid_Q": "40", "actual_sold_X": "4",
        "sale_Q": "57", "net_Q_at_price0": "17", "own_final_X": "9", "writeoff_Q": "20",
    },
    "late_unprofitable_sale_large": {
        "prior_dump_X": "150000", "prior_dump_output_Q": "900000",
        "current_pool_X": "250000", "current_pool_Q": "600000", "attempt_sale_Q": "14062",
        "result": "ProtocolRevert from final net guard", "native_fee_Q": "170",
        "initial_Q": "100000", "final_Q": "99830", "borrower_collateral_X": "6000",
        "cash_Q": "20000", "debt_Q": "80000", "claims_Q": "100000", "canonical_writeoff_Q": "0",
    },
    "single_accrual": {"parent": ["20", "80", "100", "0"], "rate": "125000000000000000",
                       "dt": "1", "after_accrual": ["20", "90", "108", "2"],
                       "after_liquidation": ["80", "0", "80", "0"],
                       "paid_Q": "60", "loss_Q": "30", "equity_loss_Q": "2", "claims_loss_Q": "28"},
    "oracle_boundary": {"max_age": "60", "preview_age_60": "Fresh", "preview_age_61": "Skip StaleOracle"},
    "flash_native_reserve": {"own_Q": "499", "resource_limit": "500", "resource_price": "1",
                             "result": "Skip; flash principal cannot fund pre-execution native reserve"},
}, indent=2))
