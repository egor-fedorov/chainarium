"""Proposed DEC-22: exact pre-implementation bundle/builder expectations.

Independent integer accounting of native transfers and a late reverted call.
No simulator imports or claim of reviewing the future implementation.
"""
import json
transfer_units = 100 + 10 + 5
late_revert_units = transfer_units + 10
bid = 50
fees = transfer_units + late_revert_units + transfer_units
sender = 1000 - 20 - fees - bid
assert (transfer_units, late_revert_units, fees, sender) == (115, 125, 355, 575)
# Each declared limit must fit immediately before that member. Actual used is charged.
required_block_limit = max(200, transfer_units + 200, transfer_units + late_revert_units + 200)
assert required_block_limit == 440
ordinary_fee = 3 * transfer_units
assert ordinary_fee == 345
print(json.dumps({
    'status': 'pre-implementation expectations for Proposed DEC-22',
    'allowed_late_revert': {
        'member_units': [str(transfer_units), str(late_revert_units), str(transfer_units)],
        'member_statuses': ['Success', 'ProtocolRevert', 'Success'],
        'native_fees_Q': str(fees), 'bid_Q': str(bid), 'builder_delta_Q': str(fees + bid),
        'sender_Q': str(sender), 'recipient_Q': '20', 'sender_nonce': '3',
    },
    'forbidden_late_revert': {
        'canonical_receipts': '0', 'canonical_fee_Q': '0',
        'sender_Q': '1000', 'recipient_Q': '0', 'builder_Q': '0', 'sender_nonce': '0',
    },
    'declared_limit_boundary': {
        'each_member_limit': '200', 'total_actual_used': str(fees),
        'minimum_initial_block_remaining': str(required_block_limit),
        'remaining_439': 'whole candidate fails inclusion; no canonical effects',
        'remaining_440': 'all three members fit',
    },
    'same_input_builder_comparison': {
        'block_limit': '500',
        'greedy': {'chosen': 'bundle', 'used': '355', 'builder_delta_Q': '405',
                   'bundle_sender_Q': '575', 'ordinary_sender_Q': '1000', 'recipient_Q': '20'},
        'fee_priority': {'chosen': 'ordinary_price_3', 'used': '115', 'builder_delta_Q': str(ordinary_fee),
                         'bundle_sender_Q': '1000', 'ordinary_sender_Q': str(1000 - 1 - ordinary_fee), 'recipient_Q': '1'},
    },
    'shared_member': {
        'included_TxId_count': '1', 'competing_bundle': 'not selected after shared member commits',
    },
    'ordinary_builder_transfer': {'body_transfer_Q': '100', 'native_fee_Q': '115',
                                  'actual_builder_delta_Q': '215', 'greedy_score_Q': '115'},
    'self_fee_only': {'receipt_fee_Q': '300', 'actual_builder_delta_Q': '0', 'greedy_score_Q': '0'},
}, indent=2))
