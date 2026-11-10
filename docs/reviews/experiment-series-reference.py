"""Proposed DEC-29: exact paired-series expectations before implementation.

Independent Python Fraction statistics and the existing scalar ChaCha12 reference;
no Chainarium imports, run outputs or production aggregation code. The four-case
builder fixture is deterministic sensitivity analysis, not a random sample.
"""
from fractions import Fraction as F
from pathlib import Path
import json
import runpy

Stream = runpy.run_path(str(Path(__file__).with_name('baseline-agents-reference.py')))['Stream']


def wire(n):
    n = F(n)
    return dict(numerator=str(n.numerator), denominator=str(n.denominator),
                floor_atoms=str(n.numerator // n.denominator))


def stats(values):
    values = sorted(map(F, values))
    n = len(values)
    if not n:
        return dict(n='0', status='Unavailable', reason='NoPricedObservations')
    mean = sum(values, F(0)) / n
    median = values[n // 2] if n % 2 else (values[n // 2 - 1] + values[n // 2]) / 2
    def nearest_rank(numerator, denominator):
        return values[(numerator * n + denominator - 1) // denominator - 1]
    return dict(n=str(n), status='Available', minimum=wire(values[0]), maximum=wire(values[-1]),
                mean=wire(mean), median=wire(median), p05=wire(nearest_rank(5, 100)),
                p95=wire(nearest_rank(95, 100)),
                sample_variance=None if n == 1 else wire(sum((x - mean)**2 for x in values) / (n - 1)),
                variance_reason='InsufficientSample' if n == 1 else None,
                positive=str(sum(x > 0 for x in values)), zero=str(sum(x == 0 for x in values)),
                negative=str(sum(x < 0 for x in values)))


def paired(rows):
    deltas, excluded = [], []
    for seed, baseline, alternative in rows:
        if baseline is None or alternative is None:
            excluded.append(str(seed))
        else:
            deltas.append(F(alternative) - F(baseline))
    return dict(requested=str(len(rows)), eligible=str(len(deltas)), excluded_seeds=excluded,
                deltas=[wire(x) for x in deltas], statistics=stats(deltas))


def random_case(seed, probability):
    rng = Stream(seed, 'agent/trader')
    transfers = fees = submissions = 0
    choices = []
    for round_ in range(1, 7):
        gate, choice = rng.uniform(1000000), rng.uniform(2)
        amount = [10, 20][choice]
        submit = gate < probability
        if submit:
            transfers += amount
            fees += 115
            submissions += 1
        choices.append(dict(round=str(round_), gate=str(gate), choice=str(choice), submit=submit))
    # Own Q5000 covers even six full fee reserves; no market/debt movement.
    return dict(seed=str(seed), pnl=wire(-transfers-fees), paid_native_fees=str(fees),
                transfers=str(transfers), included_success=str(submissions),
                included_failure='0', held=str(6-submissions), rng_words=str(rng.position),
                choices=choices)


def cases():
    builder = []
    for seed, expensive_price in enumerate([2, 3, 4, 5]):
        baseline, alternative = -10-115, -10-115*expensive_price
        builder.append(dict(seed=str(seed), expensive_price=str(expensive_price),
            baseline_pnl=wire(baseline), alternative_pnl=wire(alternative),
            delta=wire(alternative-baseline), baseline_fee='115', alternative_fee=str(115*expensive_price),
            included_success='1', included_failure='0', losing_tx='StaleNonce'))
    random = [(random_case(seed, 0), random_case(seed, 500000)) for seed in range(20)]
    return {
        'builder_four_case_sensitivity': dict(rows=builder, paired=paired([
            (int(r['seed']), F(r['baseline_pnl']['numerator']), F(r['alternative_pnl']['numerator'])) for r in builder])),
        'random_policy_twenty_seeds': dict(baseline_probability_ppm='0', alternative_probability_ppm='500000',
            initial_own_Q='5000', rounds='6', rows=[dict(baseline=a, alternative=b) for a,b in random],
            paired=paired([(seed, 0, F(b['pnl']['numerator'])) for seed,(_,b) in enumerate(random)])),
        'fractional_even_median': stats([F(-1,2),F(1,2),F(3,2),F(5,2)]),
        'paired_missing_is_not_zero': paired([(0,0,5),(1,None,100),(2,4,None),(3,10,7)]),
        'different_available_cohorts': dict(baseline=stats([0,4,10]), alternative=stats([5,100,7]),
            paired=paired([(0,0,5),(1,None,100),(2,4,None),(3,10,7)])),
        'one_observation': stats([F(-1,2)]),
        'no_priced_observations': stats([]),
        'included_failure_fraction': dict(success='2', protocol_revert='1', resource_exhausted='1',
            failure_fraction=wire(F(2,4)), inclusion_rejection_attempts='7',
            note='repeated rejection attempts are separate; denominator is four included receipts'),
        'no_included_receipts': dict(failure_fraction=None,reason='NoIncludedReceipts'),
        'failed_paid_transfer': dict(actual_pnl=wire(-115),paid_native_fees='115',included_failure='1',
            capital_outflow=wire(0), note='requested failed transfer900 does not become external capital'),
        'model_error_prefix': dict(run_status='ModelError',committed_blocks='1',
            canonical_prefix_pnl=wire(-125),eligible_for_final_statistics=False,
            note='committed prefix is retained for inspection; partial horizon is not a final observation'),
        'wide_negative_fraction': stats([F(-((1<<256)-1)**2,3),F(1,3)]),
    }


def reference():
    result = cases()
    b = result['builder_four_case_sensitivity']['paired']['statistics']
    assert b['mean'] == wire(F(-575,2))
    assert b['sample_variance'] == wire(F(66125,3))
    assert b['p05'] == wire(-460) and b['p95'] == wire(-115)
    assert result['fractional_even_median']['mean'] == wire(1)
    assert result['fractional_even_median']['median'] == wire(1)
    assert result['fractional_even_median']['sample_variance'] == wire(F(5,3))
    assert result['paired_missing_is_not_zero']['statistics']['mean'] == wire(1)
    assert result['paired_missing_is_not_zero']['excluded_seeds'] == ['1','2']
    assert all(a['rng_words'] == b['rng_words'] == '24' for a,b in
               [(r['baseline'],r['alternative']) for r in result['random_policy_twenty_seeds']['rows']])
    return dict(status='Proposed', decision='DEC-29', model='paired-experiments-v1', cases=result)


if __name__ == '__main__':
    print(json.dumps(reference(), ensure_ascii=False, indent=2))
