use super::*;

#[test]
fn flash_trial_or_commit_error_rolls_back_all_families_and_rng() {
    let mut s = Scenario::parse(include_bytes!(
        "../../../../../scenarios/demos/sc-flash-arb-success.json"
    ))
    .unwrap();
    let users = Scenario::parse(include_bytes!(
        "../../../../../scenarios/demos/sc-agent-random.json"
    ))
    .unwrap();
    let mut user = users.baseline_policies["trader"].clone();
    user.account = "bob".into();
    s.baseline_policies.insert("recipient".into(), user);
    s.model_versions.push(policy::agents::MODEL.into());
    s.scheduler.as_mut().unwrap().rounds = 6;
    s.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("Q".into(), 1000.into());
    let parent = World::genesis(&s).unwrap();
    let previous = State::default();
    let mut rng = crate::run::Streams::new(s.seed);
    let before = (parent.clone(), previous.clone(), rng.clone());
    let error = advance_with_rng_check(&s, &parent, &previous, &mut rng, |_| Ok(()), &|w| {
        if w.balance("alice", "Q") > 1000.into() {
            assert_eq!(w.balance("loan", "Q"), 10000050.into());
            return Err("fault after successful flash probe".into());
        }
        Ok(())
    })
    .unwrap_err();
    assert!(error.contains("flash probe"));
    assert_eq!((parent.clone(), previous.clone(), rng.clone()), before);
    let error = advance_with_rng_check(
        &s,
        &parent,
        &previous,
        &mut rng,
        |b| {
            assert_eq!(b.state.balance("alice", "Q"), 25104.into());
            assert_eq!(b.state.balance("loan", "Q"), 10000050.into());
            Err("fault after canonical flash settlement".into())
        },
        &|_| Ok(()),
    )
    .unwrap_err();
    assert!(error.contains("canonical flash"));
    assert_eq!((parent.clone(), previous.clone(), rng.clone()), before);
    let (block, _) = advance(&s, &parent, &previous, &mut rng).unwrap();
    assert_eq!(block.state.balance("alice", "Q"), 25104.into());
    assert_eq!(rng.position("agent/recipient"), 4.into());
}
