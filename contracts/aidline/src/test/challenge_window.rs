// ─── Issue #115: challenge window before a milestone is paid ─────────────────

use super::*;
use crate::MAX_CHALLENGE_WINDOW;

const WINDOW: u64 = 2 * DAY;

fn windowed(s: &Setup, window: u64) -> u64 {
    s.client.create_campaign_with_challenge_window(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://challenge"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 300, 300, 400],
        &window,
    )
}

fn jump(s: &Setup, seconds: u64) {
    let now = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(now + seconds);
}

fn funded(s: &Setup, window: u64) -> u64 {
    let id = windowed(s, window);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);
    id
}

#[test]
fn funds_move_only_when_the_proposal_is_finalized_after_the_window() {
    let s = Setup::new();
    let id = funded(&s, WINDOW);

    assert_eq!(
        s.client
            .propose_milestone_approval(&id, &s.verifier, &s.proof()),
        0
    );
    assert!(!s.env.events().all().events().is_empty(), "MilestoneProposed");
    assert_eq!(s.token.balance(&s.beneficiary), 0);
    assert_eq!(s.client.get_campaign(&id).milestones_released, 0);
    assert!(s.client.pending_approval(&id).is_some());

    assert_eq!(
        s.client.try_finalize_milestone(&id),
        Err(Ok(Error::WindowNotElapsed))
    );

    jump(&s, WINDOW);
    assert_eq!(s.client.finalize_milestone(&id), 300);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert_eq!(s.client.get_campaign(&id).milestones_released, 1);
    assert!(s.client.pending_approval(&id).is_none());
    assert_eq!(
        s.client.try_finalize_milestone(&id),
        Err(Ok(Error::NoProposal))
    );
}

#[test]
fn approve_milestone_cannot_bypass_the_window_or_stack_proposals() {
    let s = Setup::new();
    let id = funded(&s, WINDOW);

    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.token.balance(&s.beneficiary), 0);
    assert!(s.client.pending_approval(&id).is_some());

    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::ProposalPending))
    );
}

#[test]
fn a_window_of_zero_pays_immediately_like_before() {
    let s = Setup::new();
    let id = funded(&s, 0);

    assert_eq!(s.client.challenge_window(&id), 0);
    assert_eq!(
        s.client
            .try_propose_milestone_approval(&id, &s.verifier, &s.proof()),
        Err(Ok(Error::NoChallengeWindow))
    );
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert!(s.client.pending_approval(&id).is_none());
}

#[test]
fn the_admin_can_block_and_the_approval_can_be_proposed_again() {
    let s = Setup::new();
    let id = funded(&s, WINDOW);
    s.client.approve_milestone(&id, &s.proof());

    assert!(s.client.block_milestone(&s.admin, &id));
    assert!(!s.env.events().all().events().is_empty(), "MilestoneBlocked");
    assert!(s.client.pending_approval(&id).is_none());
    assert_eq!(s.token.balance(&s.beneficiary), 0);
    assert_eq!(
        s.client.try_finalize_milestone(&id),
        Err(Ok(Error::NoProposal))
    );

    // Back to pending: the verifier proposes it again and it can finalize.
    s.client.approve_milestone(&id, &s.proof());
    jump(&s, WINDOW);
    s.client.finalize_milestone(&id);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
}

#[test]
fn donors_block_by_weighted_quorum_once_each() {
    let s = Setup::new();
    let id = windowed(&s, WINDOW);
    let (a, b, c) = (s.donor(200), s.donor(150), s.donor(650));
    s.client.donate(&a, &id, &200);
    s.client.donate(&b, &id, &150);
    s.client.donate(&c, &id, &650);
    s.client.approve_milestone(&id, &s.proof());

    // The quorum is 30% of 1000 raised: 200 is not enough, and counts once.
    assert!(!s.client.block_milestone(&a, &id));
    assert_eq!(
        s.client.try_block_milestone(&a, &id),
        Err(Ok(Error::AlreadyChallenged))
    );
    assert!(s.client.pending_approval(&id).is_some());

    let outsider = s.donor(10);
    assert_eq!(
        s.client.try_block_milestone(&outsider, &id),
        Err(Ok(Error::NothingToRefund))
    );

    // 200 + 150 = 350 reaches it.
    assert!(s.client.block_milestone(&b, &id));
    assert!(s.client.pending_approval(&id).is_none());
}

#[test]
fn blocking_after_the_window_is_too_late() {
    let s = Setup::new();
    let id = funded(&s, WINDOW);
    s.client.approve_milestone(&id, &s.proof());

    jump(&s, WINDOW);
    assert_eq!(
        s.client.try_block_milestone(&s.admin, &id),
        Err(Ok(Error::WindowClosed))
    );
    s.client.finalize_milestone(&id);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
}

#[test]
fn the_window_has_a_maximum() {
    let s = Setup::new();
    assert!(s.client.try_create_campaign_with_challenge_window(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://max"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 300, 300, 400],
        &MAX_CHALLENGE_WINDOW,
    )
    .is_ok());
    assert_eq!(
        s.client.try_create_campaign_with_challenge_window(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://toolong"),
            &(s.env.ledger().timestamp() + 30 * DAY),
            &vec![&s.env, 300, 300, 400],
            &(MAX_CHALLENGE_WINDOW + 1),
        ),
        Err(Ok(Error::InvalidChallengeWindow))
    );
}
