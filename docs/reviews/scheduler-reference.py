"""Proposed DEC-21 expectations pinned before scheduler implementation.

Plain integer event-time arithmetic, CPMM formulas and native transfer costs.
This calculation is not a review of the future scheduler or builder.
"""
import json

send, observer_delay, builder_delay, reaction_delay = 7, 1, 2, 1
visible = send + observer_delay
reaction = visible + reaction_delay
victim_arrival = send + builder_delay
response_arrival = reaction + 1
assert (visible, reaction, victim_arrival, response_arrival) == (8, 9, 9, 10)
victim_x = 1000 * 200 // (1000 + 200)
ax, aq = 1000 - victim_x, 1200
response_x = 1000 * 100 // 1100
response_q = aq * response_x // (ax + response_x)
ax += response_x
aq -= response_q
assert (victim_x, response_x, response_q, ax, aq) == (166, 90, 116, 924, 1084)
transfer_cost = 100 + 10 + 5
assert 1000 - 20 - 10 - 2 * transfer_cost == 740

print(json.dumps({
    'status': 'pre-implementation expectations for Proposed DEC-21',
    'inclusive_cutoff': {
        'cutoff': '10', 'victim_sent_at': str(send),
        'victim_builder_arrival': str(victim_arrival), 'visible_at': str(visible),
        'reaction_at': str(reaction), 'response_builder_arrival': str(response_arrival),
        'delayed_response_arrival': str(reaction + 2), 'next_block_timestamp': '20',
        'response_deadline_with_lifetime_30': str(reaction + 30),
    },
    'public_response': {
        'victim_Q_in': '200', 'victim_X_out': str(victim_x),
        'response_Q_in': '100', 'response_X_mid': str(response_x),
        'response_Q_out': str(response_q), 'response_net_Q': str(response_q - 100),
        'pool_A_X_Q': [str(ax), str(aq)], 'pool_B_X_Q': ['910', '1100'],
        'victim_units': '120', 'response_units': '160', 'block_used': '280',
    },
    'nonce_gap': {
        'included_nonces': ['0', '1'], 'sender_Q': '740', 'recipient_Q': '30',
        'builder_Q': str(2 * transfer_cost), 'next_nonce': '2', 'declared_limits': ['200', '200'],
        'block_limit': '500',
    },
    'same_nonce_conflict': {
        'fifo': {'winner_price': '1', 'sender_Q': str(1000 - 10 - transfer_cost), 'fee_Q': str(transfer_cost)},
        'fee_priority': {'winner_price': '2', 'sender_Q': str(1000 - 10 - 2 * transfer_cost), 'fee_Q': str(2 * transfer_cost)},
        'loser_fee': '0',
    },
}, indent=2))
