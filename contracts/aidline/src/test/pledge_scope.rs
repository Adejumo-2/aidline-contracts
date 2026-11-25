// ─── Issue #125: pledge settlement is scoped to the campaign ─────────────────

use super::*;

fn pledged_campaign(s: &Setup, pledger: &Address, pledges: u32) -> u64 {
    let id = s.campaign();
    for _ in 0..pledges {
        s.client.create_pledge(pledger, &id, &1);
    }
    id
}

fn pledger(s: &Setup) -> Address {
    let p = s.donor(1_000_000);
    let expiry = s.env.ledger().sequence() + 100_000;
    s.token.approve(&p, &s.client.address, &1_000_000, &expiry);
    p
}

/// CPU instructions spent approving the first milestone of campaign 0 when
/// `other_campaigns` more campaigns each hold `pledges_each` pledges.
fn approve_cost(own_pledges: u32, other_campaigns: u32, pledges_each: u32) -> u64 {
    let s = Setup::new();
    let p = pledger(&s);
    let target = pledged_campaign(&s, &p, own_pledges);
    for _ in 0..other_campaigns {
        pledged_campaign(&s, &p, pledges_each);
    }
    let donor = s.donor(1_000);
    s.client.donate(&donor, &target, &300);

    s.env.cost_estimate().budget().reset_default();
    s.client.approve_milestone(&target, &s.proof());
    s.env.cost_estimate().budget().cpu_instruction_cost()
}

#[test]
fn approving_one_campaign_does_not_scan_other_campaigns_pledges() {
    let alone = approve_cost(5, 0, 0);
    // 5 own pledges plus 450 pledges spread over 9 unrelated campaigns.
    let crowded = approve_cost(5, 9, 50);

    assert!(
        crowded <= alone * 4,
        "approving cost {crowded} instructions with other campaigns' pledges, {alone} without"
    );
}

#[test]
fn pledge_settlement_still_pulls_the_campaigns_own_pledges() {
    let s = Setup::new();
    let p = pledger(&s);
    let id = pledged_campaign(&s, &p, 3);
    let other = pledged_campaign(&s, &p, 3);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &300);

    s.client.approve_milestone(&id, &s.proof());

    // The 3 pledges of `id` are pulled; the other campaign's are untouched.
    assert_eq!(s.client.get_campaign(&id).raised, 303);
    assert_eq!(s.client.get_campaign(&other).raised, 0);
    assert_eq!(s.token.balance(&p), 1_000_000 - 3);
}

#[test]
fn finished_pledges_leave_the_campaign_list() {
    let s = Setup::new();
    let p = pledger(&s);
    let id = pledged_campaign(&s, &p, 2);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);

    s.client.approve_milestone(&id, &s.proof());
    // Both 1-token pledges were fully pulled, so the next approval has none
    // left to read and succeeds with only escrowed funds.
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.client.get_campaign(&id).raised, 1_002);
}
