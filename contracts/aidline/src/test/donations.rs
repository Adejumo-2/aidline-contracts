use super::*;

// ─── Issue #3 Tests: Exact-goal donations ─────────────────────────────────────

#[test]
fn three_donors_reach_goal_exactly_fourth_fails() {
    let s = Setup::new();
    let id = s.campaign(); // goal 1000, milestones [300, 300, 400]
    let d1 = s.donor(400);
    let d2 = s.donor(400);
    let d3 = s.donor(400);
    let d4 = s.donor(400);

    // Three donors contribute 300 + 300 + 400 = 1000 (exactly the goal).
    s.client.donate(&d1, &id, &300);
    s.client.donate(&d2, &id, &300);
    s.client.donate(&d3, &id, &400);

    let c = s.client.get_campaign(&id);
    assert_eq!(c.raised, 1000);
    assert_eq!(c.raised, c.goal);

    // A fourth donation of any amount must fail with GoalExceeded.
    assert_eq!(
        s.client.try_donate(&d4, &id, &1),
        Err(Ok(Error::GoalExceeded))
    );
    assert_eq!(
        s.client.try_donate(&d4, &id, &100),
        Err(Ok(Error::GoalExceeded))
    );
}

#[test]
fn two_donors_reach_goal_exactly_campaign_fully_funded() {
    let s = Setup::new();
    // Single milestone campaign, goal 500.
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://exact"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 500],
    );
    let d1 = s.donor(300);
    let d2 = s.donor(300);

    s.client.donate(&d1, &id, &250);
    s.client.donate(&d2, &id, &250);

    let c = s.client.get_campaign(&id);
    assert_eq!(c.raised, 500);
    assert_eq!(c.raised, c.goal);

    // Campaign is exactly at goal, no room for even 1 more token.
    let d3 = s.donor(10);
    assert_eq!(
        s.client.try_donate(&d3, &id, &1),
        Err(Ok(Error::GoalExceeded))
    );

    // Milestone can be released since the goal is fully funded.
    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 500);
    assert_eq!(s.token.balance(&s.beneficiary), 500);
}
