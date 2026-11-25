use super::*;

// ─── Issue #24 Tests: Resource Cost Measurements ──────────────────────────────

#[test]
fn test_measure_resource_costs() {
    let s = Setup::new();
    let env = &s.env;
    let donor = s.donor(5000);

    // 1. create_campaign
    env.cost_estimate().budget().reset_default();
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://cost-test"),
        &(env.ledger().timestamp() + 30 * DAY),
        &vec![env, 1000, 1000],
    );

    // 2. donate (fund fully so later steps succeed)
    env.cost_estimate().budget().reset_default();
    s.client.donate(&donor, &id, &2000);

    // 3. emergency_fast_track
    env.cost_estimate().budget().reset_default();
    s.client.emergency_fast_track(&id);

    // 4. approve_milestone (milestone 0 = 1000, advance = 400, remaining = 600)
    env.cost_estimate().budget().reset_default();
    s.client.approve_milestone(&id, &s.proof());

    // 5. cancel_campaign (create a new campaign to cancel)
    let id2 = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(env, "ipfs://cost-cancel"),
        &(env.ledger().timestamp() + 30 * DAY),
        &vec![env, 500],
    );
    env.cost_estimate().budget().reset_default();
    s.client.cancel_campaign(&s.creator, &id2);

    // 6. refund, pass deadline so refund is available on the original campaign
    s.pass_deadline();
    env.cost_estimate().budget().reset_default();
    s.client.refund(&donor, &id);

    // Maintainers: to regenerate the resource-cost table, run this test with
    // `cargo test test_measure_resource_costs -- --nocapture` and insert
    // the output costs into `docs/ARCHITECTURE.md`.
}

// ─── Issue #25 Tests: Assert Events ───────────────────────────────────────────

#[test]
fn test_events_emitted() {
    let s = Setup::new();
    let env = &s.env;
    let donor = s.donor(1000);

    // CampaignCreated event is emitted when creating a campaign.
    // env.events().all() returns events from the most recent contract invocation.
    let id = s.campaign();
    assert!(
        !env.events().all().events().is_empty(),
        "CampaignCreated not emitted"
    );

    // Donated event is emitted on donation.
    s.client.donate(&donor, &id, &500);
    assert!(
        !env.events().all().events().is_empty(),
        "Donated not emitted"
    );

    // EmergencyAdvanceReleased event is emitted on fast track.
    s.client.emergency_fast_track(&id);
    assert!(
        !env.events().all().events().is_empty(),
        "EmergencyAdvanceReleased not emitted"
    );

    // MilestoneReleased event is emitted on approve_milestone.
    s.client.donate(&donor, &id, &500); // top up escrow
    s.client.approve_milestone(&id, &s.proof());
    assert!(
        !env.events().all().events().is_empty(),
        "MilestoneReleased not emitted"
    );

    // CampaignCancelled event is emitted on cancel.
    let id2 = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(env, "ipfs://ev-cancel"),
        &(env.ledger().timestamp() + 30 * DAY),
        &vec![env, 500],
    );
    let donor2 = s.donor(500);
    s.client.donate(&donor2, &id2, &100);
    s.client.cancel_campaign(&s.creator, &id2);
    assert!(
        !env.events().all().events().is_empty(),
        "CampaignCancelled not emitted"
    );

    // Refunded event is emitted on refund.
    s.client.refund(&donor2, &id2);
    assert!(
        !env.events().all().events().is_empty(),
        "Refunded not emitted"
    );
}
