#![cfg(test)]

use proptest::prelude::*;
use soroban_sdk::{
    Address, Env, IntoVal, InvokeError, String, contract, contractimpl,
    testutils::{Address as _, Events as _, Ledger, MockAuth, MockAuthInvoke},
    token::{StellarAssetClient, TokenClient},
    vec,
};

use crate::{Aidline, AidlineClient, CampaignKind, CampaignStatus, Error, MIN_DONATION_AMOUNT};

const DAY: u64 = 86_400;

struct Setup<'a> {
    env: Env,
    client: AidlineClient<'a>,
    token: TokenClient<'a>,
    admin: Address,
    verifier: Address,
    creator: Address,
    beneficiary: Address,
}

impl<'a> Setup<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        let token = TokenClient::new(&env, &sac.address());

        let contract_id = env.register(Aidline, (admin.clone(), sac.address()));
        let client = AidlineClient::new(&env, &contract_id);

        let verifier = Address::generate(&env);
        client.add_verifier(&verifier);

        Setup {
            creator: Address::generate(&env),
            beneficiary: Address::generate(&env),
            env,
            client,
            token,
            admin,
            verifier,
        }
    }
}

#[test]
#[should_panic(expected = "admin and token must be distinct")]
fn constructor_rejects_identical_admin_and_token() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = Address::generate(&env);
    env.register(Aidline, (addr.clone(), addr));
}

#[test]
fn constructor_initializes_storage_properly_with_distinct_addresses() {
    let s = Setup::new();
    assert_eq!(s.client.admin(), s.admin);
    assert!(s.client.is_token_allowed(&s.token.address));
}

impl<'a> Setup<'a> {
    fn donor(&self, balance: i128) -> Address {
        let donor = Address::generate(&self.env);
        StellarAssetClient::new(&self.env, &self.token.address).mint(&donor, &balance);
        donor
    }

    /// Campaign with three milestones of 300, 300 and 400 (goal 1000).
    fn campaign(&self) -> u64 {
        self.client.create_campaign(
            &self.creator,
            &self.beneficiary,
            &self.verifier,
            &self.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&self.env, "ipfs://flood-relief"),
            &(self.env.ledger().timestamp() + 30 * DAY),
            &vec![&self.env, 300, 300, 400],
        )
    }

    /// Campaign with three milestones of 300, 300 and 400 (goal 1000) and a minimum donation.
    fn campaign_with_min_donation(&self, min_donation: i128) -> u64 {
        self.client.create_campaign_min_donation(
            &self.creator,
            &self.beneficiary,
            &self.verifier,
            &self.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&self.env, "ipfs://flood-relief"),
            &(self.env.ledger().timestamp() + 30 * DAY),
            &vec![&self.env, 300, 300, 400],
            &min_donation,
        )
    }

    /// Campaign with three milestones of 300, 300 and 400 (goal 1000) and an approval cooldown.
    fn campaign_with_cooldown(&self, cooldown: u64) -> u64 {
        self.client.create_campaign_with_cooldown(
            &self.creator,
            &self.beneficiary,
            &self.verifier,
            &self.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&self.env, "ipfs://flood-relief"),
            &(self.env.ledger().timestamp() + 30 * DAY),
            &vec![&self.env, 300, 300, 400],
            &cooldown,
        )
    }

    fn proof(&self) -> String {
        String::from_str(&self.env, "ipfs://proof")
    }

    fn pass_deadline(&self) {
        let now = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(now + 31 * DAY);
    }
}

#[test]
fn creates_campaign_with_goal_from_milestones() {
    let s = Setup::new();
    let id = s.campaign();

    let c = s.client.get_campaign(&id);
    assert_eq!(id, 0);
    assert_eq!(c.goal, 1000);
    assert_eq!(c.status, CampaignStatus::Active);
    assert_eq!(c.kind, CampaignKind::Emergency);
    assert_eq!(s.client.campaign_count(), 1);
}

#[test]
fn rejects_unknown_verifier() {
    let s = Setup::new();
    let stranger = Address::generate(&s.env);
    let res = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &stranger,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://trees"),
        &(s.env.ledger().timestamp() + DAY),
        &vec![&s.env, 100],
    );
    assert_eq!(res, Err(Ok(Error::NotVerifier)));
}

#[test]
fn rejects_conflict_of_interest() {
    let s = Setup::new();
    let future = s.env.ledger().timestamp() + DAY;
    let uri = String::from_str(&s.env, "ipfs://trees");

    // Verifier is also creator
    let res_creator = s.client.try_create_campaign(
        &s.verifier,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &future,
        &vec![&s.env, 100],
    );
    assert_eq!(res_creator, Err(Ok(Error::ConflictOfInterest)));

    // Verifier is also beneficiary
    let res_beneficiary = s.client.try_create_campaign(
        &s.creator,
        &s.verifier,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &future,
        &vec![&s.env, 100],
    );
    assert_eq!(res_beneficiary, Err(Ok(Error::ConflictOfInterest)));
}

#[test]
fn rejects_bad_milestones_and_deadline() {
    let s = Setup::new();
    let uri = String::from_str(&s.env, "ipfs://x");
    let future = s.env.ledger().timestamp() + DAY;

    let empty = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &future,
        &vec![&s.env],
    );
    assert_eq!(empty, Err(Ok(Error::InvalidMilestones)));

    let negative = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &future,
        &vec![&s.env, 100, -5],
    );
    assert_eq!(negative, Err(Ok(Error::InvalidMilestones)));

    let past = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &s.env.ledger().timestamp(),
        &vec![&s.env, 100],
    );
    assert_eq!(past, Err(Ok(Error::DeadlineInPast)));
}

#[test]
fn rejects_invalid_metadata_uri() {
    let s = Setup::new();
    let future = s.env.ledger().timestamp() + DAY;

    // Empty metadata URI should fail with InvalidMetadataUri
    let empty_uri = String::from_str(&s.env, "");
    let res_empty = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &empty_uri,
        &future,
        &vec![&s.env, 100],
    );
    assert_eq!(res_empty, Err(Ok(Error::InvalidMetadataUri)));

    // 501 bytes should fail with InvalidMetadataUri
    let long_str = "a".repeat(501);
    let long_uri = String::from_str(&s.env, &long_str);
    let res_long = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &long_uri,
        &future,
        &vec![&s.env, 100],
    );
    assert_eq!(res_long, Err(Ok(Error::InvalidMetadataUri)));

    // Exact limit 500 bytes should succeed
    let max_str = "a".repeat(500);
    let max_uri = String::from_str(&s.env, &max_str);
    let ok_res = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &max_uri,
        &future,
        &vec![&s.env, 100],
    );
    assert!(ok_res.is_ok());

    // Valid IPFS URI succeeds
    let ipfs_uri = String::from_str(&s.env, "ipfs://bafybeicg2gg33zrqb33rqd6dffq72u7i7743rqd6d");
    let ok_ipfs = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &ipfs_uri,
        &future,
        &vec![&s.env, 100],
    );
    assert!(ok_ipfs.is_ok());

    // create_campaign_with_due_dates also validates metadata_uri
    let res_due_empty = s.client.try_create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &empty_uri,
        &future,
        &vec![&s.env, 100],
        &vec![&s.env, future - 10],
    );
    assert_eq!(res_due_empty, Err(Ok(Error::InvalidMetadataUri)));

    let res_due_long = s.client.try_create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &long_uri,
        &future,
        &vec![&s.env, 100],
        &vec![&s.env, future - 10],
    );
    assert_eq!(res_due_long, Err(Ok(Error::InvalidMetadataUri)));
}

#[test]
fn rejects_invalid_proof_uri() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // Empty proof URI should fail with InvalidProofUri
    let empty_proof = String::from_str(&s.env, "");
    let res_empty = s.client.try_approve_milestone(&id, &empty_proof);
    assert_eq!(res_empty, Err(Ok(Error::InvalidProofUri)));

    // 501 bytes should fail with InvalidProofUri
    let long_str = "p".repeat(501);
    let long_proof = String::from_str(&s.env, &long_str);
    let res_long = s.client.try_approve_milestone(&id, &long_proof);
    assert_eq!(res_long, Err(Ok(Error::InvalidProofUri)));

    // Exact limit 500 bytes should succeed (releases milestone 0: 300)
    let max_str = "p".repeat(500);
    let max_proof = String::from_str(&s.env, &max_str);
    let ok_res = s.client.try_approve_milestone(&id, &max_proof);
    assert_eq!(ok_res, Ok(Ok(300)));

    // Valid IPFS URI should succeed (releases milestone 1: 300)
    let ipfs_proof = String::from_str(
        &s.env,
        "ipfs://bafybeicg2gg33zrqb33rqd6dffq72u7i7743rqd6dffq72u7i7743rqd6d",
    );
    let ok_ipfs = s.client.try_approve_milestone(&id, &ipfs_proof);
    assert_eq!(ok_ipfs, Ok(Ok(300)));

    // Valid HTTPS URI should succeed (releases milestone 2: 400)
    let https_proof = String::from_str(&s.env, "https://aidline.org/proofs/milestone-3.pdf");
    let ok_https = s.client.try_approve_milestone(&id, &https_proof);
    assert_eq!(ok_https, Ok(Ok(400)));
}

#[test]
fn donations_are_held_in_escrow() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);

    s.client.donate(&donor, &id, &200);
    s.client.donate(&donor, &id, &100);

    assert_eq!(s.token.balance(&donor), 200);
    assert_eq!(s.token.balance(&s.client.address), 300);
    assert_eq!(s.client.contribution_of(&id, &donor), 300);
    assert_eq!(s.client.get_campaign(&id).raised, 300);
}

#[test]
fn donation_accepts_partial_amount_when_passing_goal() {
    let s = Setup::new();
    let id = s.campaign(); // goal 1000
    let donor = s.donor(2000);

    // Donating 1500 to a 1000-goal campaign accepts 1000, transfers only 1000, returns 1000
    let accepted = s.client.donate(&donor, &id, &1500);
    assert_eq!(accepted, 1000);
    assert_eq!(s.token.balance(&donor), 1000);
    assert_eq!(s.token.balance(&s.client.address), 1000);
    assert_eq!(s.client.get_campaign(&id).raised, 1000);
    assert_eq!(s.client.contribution_of(&id, &donor), 1000);

    // Fully funded campaign now rejects subsequent donations
    assert_eq!(
        s.client.try_donate(&donor, &id, &10),
        Err(Ok(Error::GoalExceeded))
    );
    assert_eq!(
        s.client.try_donate(&donor, &id, &0),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn donation_partial_acceptance_with_existing_raised() {
    let s = Setup::new();
    let id = s.campaign(); // goal 1000
    let donor_a = s.donor(1000);
    let donor_b = s.donor(1000);

    // Donor A donates 700 (exact)
    let accepted_a = s.client.donate(&donor_a, &id, &700);
    assert_eq!(accepted_a, 700);
    assert_eq!(s.token.balance(&donor_a), 300);

    // Donor B offers 500 when only 300 is needed
    let accepted_b = s.client.donate(&donor_b, &id, &500);
    assert_eq!(accepted_b, 300);
    assert_eq!(s.token.balance(&donor_b), 700); // only 300 deducted
    assert_eq!(s.client.get_campaign(&id).raised, 1000);
    assert_eq!(s.client.contribution_of(&id, &donor_b), 300);

    // Now campaign is fully funded, rejects any further donations
    let donor_c = s.donor(100);
    assert_eq!(
        s.client.try_donate(&donor_c, &id, &50),
        Err(Ok(Error::GoalExceeded))
    );
}

#[test]
fn milestones_release_in_order_and_complete() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    assert_eq!(s.client.approve_milestone(&id, &s.proof()), 300);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert_eq!(s.client.approve_milestone(&id, &s.proof()), 300);
    assert_eq!(s.client.approve_milestone(&id, &s.proof()), 400);

    let c = s.client.get_campaign(&id);
    assert_eq!(s.token.balance(&s.beneficiary), 1000);
    assert_eq!(c.released, 1000);
    assert_eq!(c.status, CampaignStatus::Completed);
    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn milestone_needs_enough_funds() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &299);

    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::MilestoneNotFunded))
    );
}

#[test]
fn removed_verifier_cannot_approve() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);
    s.client.remove_verifier(&s.verifier);

    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::NotVerifier))
    );
}

#[test]
fn cancelled_campaign_refunds_unreleased_pro_rata() {
    let s = Setup::new();
    let id = s.campaign();
    let alice = s.donor(1000);
    let bob = s.donor(1000);
    s.client.donate(&alice, &id, &600);
    s.client.donate(&bob, &id, &400);
    s.client.approve_milestone(&id, &s.proof()); // 300 released, 700 left

    s.client.cancel_campaign(&s.creator, &id);

    assert_eq!(s.client.refund(&alice, &id), 420);
    assert_eq!(s.client.refund(&bob, &id), 280);
    assert_eq!(s.token.balance(&s.client.address), 0);
    assert_eq!(
        s.client.try_refund(&alice, &id),
        Err(Ok(Error::NothingToRefund))
    );
}

#[test]
fn expired_campaign_allows_refund_and_blocks_activity() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &500);

    assert_eq!(
        s.client.try_refund(&donor, &id),
        Err(Ok(Error::RefundNotAvailable))
    );

    s.pass_deadline();
    assert_eq!(
        s.client.try_donate(&donor, &id, &10),
        Err(Ok(Error::CampaignExpired))
    );
    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignExpired))
    );
    assert_eq!(s.client.refund(&donor, &id), 500);
    assert_eq!(s.token.balance(&donor), 1000);
}

// ─── refund_for tests ────────────────────────────────────────────────────────

#[test]
fn refund_for_pays_donor_never_caller() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    let relayer = Address::generate(&s.env);

    s.client.donate(&donor, &id, &600);
    s.client.cancel_campaign(&s.creator, &id);

    // Initial relayer balance is 0
    assert_eq!(s.token.balance(&relayer), 0);
    assert_eq!(s.token.balance(&donor), 400);

    // Relayer triggers refund on behalf of donor without donor authorization
    let auth_relayer = MockAuth {
        address: &relayer,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "refund_for",
            args: (donor.clone(), id).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let refunded = s.client.mock_auths(&[auth_relayer]).refund_for(&donor, &id);
    assert_eq!(refunded, 600);

    // Tokens were paid to donor, NOT relayer
    assert_eq!(s.token.balance(&donor), 1000);
    assert_eq!(s.token.balance(&relayer), 0);
}

#[test]
fn refund_for_prevents_double_refund() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);

    s.client.donate(&donor, &id, &500);
    s.client.cancel_campaign(&s.creator, &id);

    // First refund succeeds
    let refunded = s.client.refund_for(&donor, &id);
    assert_eq!(refunded, 500);

    // Second refund via refund_for fails with NothingToRefund
    let second_res = s.client.try_refund_for(&donor, &id);
    assert_eq!(second_res, Err(Ok(Error::NothingToRefund)));

    // Second refund via refund fails with NothingToRefund
    let direct_refund_res = s.client.try_refund(&donor, &id);
    assert_eq!(direct_refund_res, Err(Ok(Error::NothingToRefund)));
}

#[test]
fn refund_for_matches_refund_errors_on_non_refundable_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);

    s.client.donate(&donor, &id, &300);

    // Campaign is active and on schedule: RefundNotAvailable
    let res_active = s.client.try_refund_for(&donor, &id);
    assert_eq!(res_active, Err(Ok(Error::RefundNotAvailable)));

    // Unknown campaign: CampaignNotFound
    let res_not_found = s.client.try_refund_for(&donor, &999_999);
    assert_eq!(res_not_found, Err(Ok(Error::CampaignNotFound)));

    // Cancel campaign, but test non-contributor
    s.client.cancel_campaign(&s.creator, &id);
    let non_contributor = Address::generate(&s.env);
    let res_nothing = s.client.try_refund_for(&non_contributor, &id);
    assert_eq!(res_nothing, Err(Ok(Error::NothingToRefund)));
}

#[test]
fn refund_for_requires_no_donor_auth() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);

    s.client.donate(&donor, &id, &400);
    s.client.cancel_campaign(&s.creator, &id);

    // Invocation with empty mock_auths (no auth whatsoever provided)
    let refunded = s.client.mock_auths(&[]).refund_for(&donor, &id);
    assert_eq!(refunded, 400);
    assert_eq!(s.token.balance(&donor), 1000);
}

#[test]
fn is_refundable_covers_all_statuses() {
    let s = Setup::new();
    let id = s.campaign();

    // 1. Active campaign within deadline is not refundable
    assert!(!s.client.is_refundable(&id));

    // 2. Cancelled campaign is refundable
    let cancel_id = s.campaign();
    s.client.cancel_campaign(&s.creator, &cancel_id);
    assert_eq!(
        s.client.get_campaign(&cancel_id).status,
        CampaignStatus::Cancelled
    );
    assert!(s.client.is_refundable(&cancel_id));

    // 3. Expired active campaign is refundable
    let expired_id = s.campaign();
    s.pass_deadline();
    assert_eq!(
        s.client.get_campaign(&expired_id).status,
        CampaignStatus::Active
    );
    assert!(s.client.is_refundable(&expired_id));

    // 4. Overdue milestone in active campaign is refundable
    let s2 = Setup::new();
    let now = s2.env.ledger().timestamp();
    let due_dates = vec![&s2.env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![&s2.env, 300, 700];
    let overdue_id = s2.client.create_campaign_with_due_dates(
        &s2.creator,
        &s2.beneficiary,
        &s2.verifier,
        &s2.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s2.env, "ipfs://overdue"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );
    assert!(!s2.client.is_refundable(&overdue_id));
    // Advance past milestone 0 due date but before campaign deadline
    s2.env.ledger().set_timestamp(now + 11 * DAY);
    assert!(s2.client.is_refundable(&overdue_id));

    // 5. Completed campaign is not refundable
    let s3 = Setup::new();
    let completed_id = s3.campaign();
    let donor = s3.donor(1000);
    s3.client.donate(&donor, &completed_id, &1000);
    s3.client.approve_milestone(&completed_id, &s3.proof()); // 300
    s3.client.approve_milestone(&completed_id, &s3.proof()); // 300
    s3.client.approve_milestone(&completed_id, &s3.proof()); // 400
    assert_eq!(
        s3.client.get_campaign(&completed_id).status,
        CampaignStatus::Completed
    );
    assert!(!s3.client.is_refundable(&completed_id));

    // 6. Non-existent campaign is not refundable
    assert!(!s3.client.is_refundable(&999_999));
}

#[test]
fn only_creator_or_admin_can_cancel() {
    let s = Setup::new();
    let id = s.campaign();
    let stranger = Address::generate(&s.env);

    assert_eq!(
        s.client.try_cancel_campaign(&stranger, &id),
        Err(Ok(Error::Unauthorized))
    );
    s.client.cancel_campaign(&s.admin, &id);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);
}

#[test]
fn missing_campaign_errors() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_get_campaign(&42),
        Err(Ok(Error::CampaignNotFound))
    );
}

// ─── Issue #23 Tests: Emergency fast track ────────────────────────────────────

#[test]
fn emergency_fast_track_works_for_emergency_campaign() {
    let s = Setup::new();
    let id = s.campaign(); // Goal 1000
    let donor = s.donor(500);

    // Donate 300. Cap is 20% of 1000 = 200.
    s.client.donate(&donor, &id, &300);

    assert_eq!(s.client.emergency_fast_track(&id), 200);
    assert_eq!(s.token.balance(&s.beneficiary), 200);

    let c = s.client.get_campaign(&id);
    assert_eq!(c.emergency_advance, 200);
    assert_eq!(c.released, 200);
}

#[test]
fn emergency_fast_track_capped_by_milestone_zero() {
    let s = Setup::new();
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://flood-relief"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 100, 400, 500],
    );
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);

    // 20% of goal is 200, but milestone 0 is only 100.
    assert_eq!(s.client.emergency_fast_track(&id), 100);
    assert_eq!(s.client.get_campaign(&id).released, 100);

    // Approving milestone 0 pays nothing more and keeps `released` intact.
    s.client.approve_milestone(&id, &s.proof());
    let c = s.client.get_campaign(&id);
    assert_eq!(c.released, 100);
    assert_eq!(c.milestones_released, 1);
    assert_eq!(s.token.balance(&s.beneficiary), 100);
    assert_eq!(s.token.balance(&s.client.address), 900);
}

#[test]
fn emergency_fast_track_limited_by_escrow() {
    let s = Setup::new();
    let id = s.campaign(); // Goal 1000
    let donor = s.donor(500);

    // Donate 100. Cap is 200, but only 100 escrowed.
    s.client.donate(&donor, &id, &100);

    assert_eq!(s.client.emergency_fast_track(&id), 100);
    assert_eq!(s.token.balance(&s.beneficiary), 100);
}

#[test]
fn emergency_fast_track_cannot_exceed_cap() {
    let s = Setup::new();
    let id = s.campaign(); // Goal 1000
    let donor = s.donor(1000);

    // Donate 1000. Cap is 200.
    s.client.donate(&donor, &id, &1000);

    assert_eq!(s.client.emergency_fast_track(&id), 200);

    // Try again -> already taken
    assert_eq!(
        s.client.try_emergency_fast_track(&id),
        Err(Ok(Error::AdvanceAlreadyTaken))
    );
}

#[test]
fn emergency_fast_track_fails_for_non_emergency() {
    let s = Setup::new();
    let donor = s.donor(1000);

    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://trees"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 1000],
    );

    s.client.donate(&donor, &id, &1000);

    assert_eq!(
        s.client.try_emergency_fast_track(&id),
        Err(Ok(Error::NotEmergencyCampaign))
    );
}

#[test]
fn emergency_fast_track_authorization() {
    let s = Setup::new();
    let id = s.campaign();

    s.client.remove_verifier(&s.verifier);
    assert_eq!(
        s.client.try_emergency_fast_track(&id),
        Err(Ok(Error::NotVerifier))
    );
}

#[test]
fn emergency_fast_track_accounting_with_milestone_one() {
    let s = Setup::new();
    let id = s.campaign(); // milestones: 300, 300, 400
    let donor = s.donor(1000);

    s.client.donate(&donor, &id, &1000);

    // Fast track takes 200
    s.client.emergency_fast_track(&id);
    assert_eq!(s.token.balance(&s.beneficiary), 200);

    // Approving milestone one (scheduled 300) should only release remaining 100
    let amt = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(amt, 300); // returns scheduled amount
    assert_eq!(s.token.balance(&s.beneficiary), 300); // 200 + 100

    let c = s.client.get_campaign(&id);
    assert_eq!(c.released, 300); // total released so far
    assert_eq!(c.emergency_advance, 200);
    assert_eq!(c.milestones_released, 1);
}

#[test]
fn emergency_fast_track_requires_escrow() {
    let s = Setup::new();
    let id = s.campaign();

    assert_eq!(
        s.client.try_emergency_fast_track(&id),
        Err(Ok(Error::AdvanceExceedsEscrow))
    );
}

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

// ─── Issue #26 Tests: Per-milestone due dates ────────────────────────────────

#[test]
fn due_date_refunds_do_not_cancel_campaign() {
    let s = Setup::new();
    let env = &s.env;

    let now = env.ledger().timestamp();
    let due_dates = vec![env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![env, 300, 700];

    let id = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://due-dates"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );

    let donor_a = s.donor(500);
    let donor_b = s.donor(500);

    s.client.donate(&donor_a, &id, &500);
    s.client.donate(&donor_b, &id, &500);

    s.client.approve_milestone(&id, &s.proof()); // 300 released

    // Advance past first milestone due date, into second milestone window.
    env.ledger().set_timestamp(now + 21 * DAY);

    // Milestone 1 is now overdue, refunds become available.
    let refund_a = s.client.refund(&donor_a, &id);
    assert_eq!(refund_a, 350); // 500 * 700 / 1000

    let c = s.client.get_campaign(&id);
    assert_eq!(c.status, CampaignStatus::Cancelled);
    assert_eq!(c.raised, 1000);
    assert_eq!(c.released, 300);

    let refund_b = s.client.refund(&donor_b, &id);
    assert_eq!(refund_b, 350);

    let c2 = s.client.get_campaign(&id);
    assert_eq!(c2.status, CampaignStatus::Cancelled);
    assert_eq!(c2.raised, 1000);
    assert_eq!(c2.released, 300);

    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn overdue_refunds_path_independent_with_three_donors() {
    let s = Setup::new();
    let env = &s.env;

    let now = env.ledger().timestamp();
    let due_dates = vec![env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![env, 300, 700];

    let id = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://asymm-refunds"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );

    let donor_a = s.donor(200);
    let donor_b = s.donor(300);
    let donor_c = s.donor(500);

    s.client.donate(&donor_a, &id, &200);
    s.client.donate(&donor_b, &id, &300);
    s.client.donate(&donor_c, &id, &500);

    s.client.approve_milestone(&id, &s.proof()); // 300 released

    // Advance time past milestone 1 due date
    env.ledger().set_timestamp(now + 21 * DAY);

    // Donor C refunds first
    let refund_c = s.client.refund(&donor_c, &id);
    assert_eq!(refund_c, 350); // 500 * 700 / 1000

    // Campaign transitioned to Cancelled and totals remain immutable
    let c = s.client.get_campaign(&id);
    assert_eq!(c.status, CampaignStatus::Cancelled);
    assert_eq!(c.raised, 1000);
    assert_eq!(c.released, 300);

    // Donor A refunds second
    let refund_a = s.client.refund(&donor_a, &id);
    assert_eq!(refund_a, 140); // 200 * 700 / 1000

    // Donor B refunds third
    let refund_b = s.client.refund(&donor_b, &id);
    assert_eq!(refund_b, 210); // 300 * 700 / 1000

    // Sum of all refunds equals raised - released
    assert_eq!(refund_a + refund_b + refund_c, 700);
    assert_eq!(refund_a + refund_b + refund_c + c.released, c.raised);

    // approve_milestone is rejected on cancelled campaign
    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn overdue_milestone_blocks_new_donations_sponsor_pools_and_pledges() {
    let s = Setup::new();
    let env = &s.env;

    let now = env.ledger().timestamp();
    let due_dates = vec![env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![env, 300, 700];

    let id = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://due-dates-overdue"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );

    let donor_a = s.donor(500);
    let donor_b = s.donor(500);
    let sponsor = s.donor(1_000);
    let pledger = s.donor(500);

    // Initial donation succeeds before milestone is overdue
    s.client.donate(&donor_a, &id, &200);

    // Advance time past milestone 0 due date
    env.ledger().set_timestamp(now + 11 * DAY);

    // New donation fails with MilestoneOverdue
    assert_eq!(
        s.client.try_donate(&donor_b, &id, &200),
        Err(Ok(Error::MilestoneOverdue))
    );

    // Deposit sponsor pool fails with MilestoneOverdue
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &1_000, &500, &500),
        Err(Ok(Error::MilestoneOverdue))
    );

    // Create pledge fails with MilestoneOverdue
    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&pledger, &s.client.address, &500, &expiry);
    assert_eq!(
        s.client.try_create_pledge(&pledger, &id, &500),
        Err(Ok(Error::MilestoneOverdue))
    );

    // Existing donor can still refund
    let refunded = s.client.refund(&donor_a, &id);
    assert_eq!(refunded, 200);
}

// ─── Issue #2 Tests: AdminChanged event ──────────────────────────────────────

#[test]
fn set_admin_emits_admin_changed_event() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);

    // set_admin emits AdminChanged. env.events().all() returns events from the
    // most recent contract invocation; after set_admin it must be non-empty.
    s.client.set_admin(&new_admin);
    assert!(
        !s.env.events().all().events().is_empty(),
        "AdminChanged event not emitted"
    );

    // The contract's admin should now be new_admin.
    assert_eq!(s.client.admin(), new_admin);
}

#[test]
fn set_admin_updates_stored_admin() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);

    let old_admin = s.client.admin();
    s.client.set_admin(&new_admin);

    assert_ne!(s.client.admin(), old_admin);
    assert_eq!(s.client.admin(), new_admin);
}

// ─── Two-step admin transfer tests ───────────────────────────────────────────

#[test]
fn propose_admin_does_nothing_until_accepted() {
    let s = Setup::new();
    let candidate = Address::generate(&s.env);

    assert_eq!(s.client.pending_admin(), None);
    s.client.propose_admin(&candidate);

    // Pending admin is recorded, but active admin remains unchanged.
    assert_eq!(s.client.pending_admin(), Some(candidate.clone()));
    assert_eq!(s.client.admin(), s.admin);

    // Accept finalizes the transfer
    s.client.accept_admin();
    assert_eq!(s.client.admin(), candidate);
    assert_eq!(s.client.pending_admin(), None);
}

#[test]
fn accept_admin_fails_when_no_pending_proposal() {
    let s = Setup::new();
    assert_eq!(s.client.pending_admin(), None);
    let res = s.client.try_accept_admin();
    assert_eq!(res, Err(Ok(Error::NoPendingAdmin)));
}

#[test]
fn new_proposal_replaces_pending_proposal() {
    let s = Setup::new();
    let candidate_a = Address::generate(&s.env);
    let candidate_b = Address::generate(&s.env);

    s.client.propose_admin(&candidate_a);
    assert_eq!(s.client.pending_admin(), Some(candidate_a.clone()));

    // Replace proposal with candidate_b
    s.client.propose_admin(&candidate_b);
    assert_eq!(s.client.pending_admin(), Some(candidate_b.clone()));

    // Candidate A cannot accept because auth for candidate B is required
    let auth_a = MockAuth {
        address: &candidate_a,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "accept_admin",
            args: ().into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res_a = s.client.mock_auths(&[auth_a]).try_accept_admin();
    assert_eq!(res_a.unwrap_err(), Err(InvokeError::Abort));

    // Candidate B can accept
    let auth_b = MockAuth {
        address: &candidate_b,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "accept_admin",
            args: ().into_val(&s.env),
            sub_invokes: &[],
        },
    };
    s.client.mock_auths(&[auth_b]).accept_admin();
    assert_eq!(s.client.admin(), candidate_b);
    assert_eq!(s.client.pending_admin(), None);
}

#[test]
fn only_pending_admin_can_accept() {
    let s = Setup::new();
    let candidate = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);

    s.client.propose_admin(&candidate);

    let auth_stranger = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "accept_admin",
            args: ().into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res = s.client.mock_auths(&[auth_stranger]).try_accept_admin();
    assert_eq!(res.unwrap_err(), Err(InvokeError::Abort));
    assert_eq!(s.client.admin(), s.admin);
}

#[test]
fn events_emitted_for_propose_and_accept() {
    let s = Setup::new();
    let candidate = Address::generate(&s.env);

    s.client.propose_admin(&candidate);
    assert!(
        !s.env.events().all().events().is_empty(),
        "AdminProposed event not emitted"
    );

    s.client.accept_admin();
    assert!(
        !s.env.events().all().events().is_empty(),
        "AdminAccepted event not emitted"
    );
}

#[test]
fn set_admin_clears_pending_proposal() {
    let s = Setup::new();
    let candidate_a = Address::generate(&s.env);
    let candidate_b = Address::generate(&s.env);

    s.client.propose_admin(&candidate_a);
    assert_eq!(s.client.pending_admin(), Some(candidate_a));

    s.client.set_admin(&candidate_b);
    assert_eq!(s.client.admin(), candidate_b);
    assert_eq!(s.client.pending_admin(), None);

    let res = s.client.try_accept_admin();
    assert_eq!(res, Err(Ok(Error::NoPendingAdmin)));
}

#[test]
fn non_admin_cannot_propose_admin() {
    let s = Setup::new();
    let stranger = Address::generate(&s.env);
    let candidate = Address::generate(&s.env);

    let auth = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "propose_admin",
            args: (candidate.clone(),).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let expected_auth_err = soroban_sdk::Error::from_type_and_code(
        soroban_sdk::xdr::ScErrorType::Context,
        soroban_sdk::xdr::ScErrorCode::InvalidAction,
    );
    let res = s.client.mock_auths(&[auth]).try_propose_admin(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), expected_auth_err);
    assert_eq!(s.client.pending_admin(), None);
}

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
        s.client.try_donate(&d4, &id, &MIN_DONATION_AMOUNT),
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
        s.client.try_donate(&d3, &id, &MIN_DONATION_AMOUNT),
        Err(Ok(Error::GoalExceeded))
    );

    // Milestone can be released since the goal is fully funded.
    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 500);
    assert_eq!(s.token.balance(&s.beneficiary), 500);
}

// ─── Issue #4 Tests: Verifier reassignment ───────────────────────────────────

#[test]
fn admin_can_reassign_verifier_on_active_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&new_v);

    // reassign_verifier emits VerifierReassigned; events() returns events from
    // the most recent invocation, must be non-empty after the call.
    s.client.reassign_verifier(&id, &new_v);
    assert!(
        !s.env.events().all().events().is_empty(),
        "VerifierReassigned event not emitted"
    );

    let c = s.client.get_campaign(&id);
    assert_eq!(c.verifier, new_v);
}

#[test]
fn reassign_fails_for_unregistered_verifier() {
    let s = Setup::new();
    let id = s.campaign();
    let stranger = Address::generate(&s.env);

    assert_eq!(
        s.client.try_reassign_verifier(&id, &stranger),
        Err(Ok(Error::NotVerifier))
    );
}

#[test]
fn reassign_verifier_rejects_campaign_beneficiary() {
    let s = Setup::new();
    let id = s.campaign();

    // Register beneficiary as a verifier in the system
    s.client.add_verifier(&s.beneficiary);

    // Admin attempts to reassign the campaign's verifier to the beneficiary
    let res = s.client.try_reassign_verifier(&id, &s.beneficiary);
    assert_eq!(res, Err(Ok(Error::VerifierCannotBeBeneficiary)));

    // Legitimate reassignment to a distinct registered verifier succeeds
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&new_v);
    assert!(s.client.try_reassign_verifier(&id, &new_v).is_ok());
    assert_eq!(s.client.get_campaign(&id).verifier, new_v);
}

#[test]
fn reassign_fails_for_cancelled_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&new_v);

    s.client.cancel_campaign(&s.admin, &id);
    assert_eq!(
        s.client.try_reassign_verifier(&id, &new_v),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn reassign_fails_for_expired_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&new_v);

    s.pass_deadline();
    assert_eq!(
        s.client.try_reassign_verifier(&id, &new_v),
        Err(Ok(Error::CampaignExpired))
    );
}

#[test]
fn new_verifier_can_approve_after_reassignment() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // Remove old verifier and reassign to a new one.
    s.client.remove_verifier(&s.verifier);
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&new_v);
    s.client.reassign_verifier(&id, &new_v);

    // New verifier controls the campaign; milestone should release.
    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 300);
    assert_eq!(s.client.get_campaign(&id).verifier, new_v);
}

#[test]
fn reassign_verifier_caps_at_maximum() {
    let s = Setup::new();
    let id = s.campaign();

    // Initial reassignment count is 0
    assert_eq!(s.client.verifier_reassignments(&id), 0);
    assert_eq!(s.client.get_campaign(&id).verifier_reassignments, 0);

    let v1 = Address::generate(&s.env);
    let v2 = Address::generate(&s.env);
    let v3 = Address::generate(&s.env);
    s.client.add_verifier(&v1);
    s.client.add_verifier(&v2);
    s.client.add_verifier(&v3);

    // Reassignment 1 succeeds
    s.client.reassign_verifier(&id, &v1);
    assert_eq!(s.client.verifier_reassignments(&id), 1);
    assert_eq!(s.client.get_campaign(&id).verifier_reassignments, 1);
    assert_eq!(s.client.get_campaign(&id).verifier, v1);

    // Reassignment 2 succeeds (reaches maximum of 2)
    s.client.reassign_verifier(&id, &v2);
    assert_eq!(s.client.verifier_reassignments(&id), 2);
    assert_eq!(s.client.get_campaign(&id).verifier_reassignments, 2);
    assert_eq!(s.client.get_campaign(&id).verifier, v2);

    // Reassignment 3 rejected with MaxVerifierReassignmentsExceeded
    let res3 = s.client.try_reassign_verifier(&id, &v3);
    assert_eq!(res3, Err(Ok(Error::MaxVerifierReassignmentsExceeded)));
    assert_eq!(s.client.verifier_reassignments(&id), 2);
    assert_eq!(s.client.get_campaign(&id).verifier_reassignments, 2);
    assert_eq!(s.client.get_campaign(&id).verifier, v2);
}

#[test]
fn verifier_reassignments_view_errors_for_nonexistent_campaign() {
    let s = Setup::new();
    let res = s.client.try_verifier_reassignments(&999_999);
    assert_eq!(res, Err(Ok(Error::CampaignNotFound)));
}

// ─── Verifier bonds (issue #29) ───────────────────────────────────────────────

use crate::BondStatus;

impl Setup<'_> {
    /// A fresh address holding `balance` tokens that has posted a bond of `bond`.
    fn bonded_verifier(&self, balance: i128, bond: i128) -> Address {
        let v = self.donor(balance);
        self.client.register_with_bond(&v, &bond);
        v
    }
}

#[test]
fn bond_below_requirement_is_rejected() {
    let s = Setup::new();
    s.client.set_bond_requirement(&500);
    let v = s.donor(1_000);
    assert_eq!(
        s.client.try_register_with_bond(&v, &499),
        Err(Ok(Error::BondRequired))
    );
    assert!(!s.client.is_verifier(&v));
    assert_eq!(s.token.balance(&v), 1_000);
}

#[test]
fn zero_bond_is_rejected_even_when_no_bond_is_required() {
    let s = Setup::new();
    let v = s.donor(1_000);
    assert_eq!(
        s.client.try_register_with_bond(&v, &0),
        Err(Ok(Error::BondRequired))
    );
}

#[test]
fn negative_bond_requirement_is_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_set_bond_requirement(&-1),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn posting_a_bond_registers_the_verifier_and_escrows_the_tokens() {
    let s = Setup::new();
    s.client.set_bond_requirement(&500);
    let v = s.bonded_verifier(1_000, 600);

    assert!(s.client.is_verifier(&v));
    assert_eq!(s.token.balance(&v), 400);
    assert_eq!(s.token.balance(&s.client.address), 600);

    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.amount, 600);
    assert_eq!(bond.remaining, 600);
    assert_eq!(bond.status, BondStatus::Active);
}

#[test]
fn an_already_registered_verifier_cannot_post_a_bond() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_register_with_bond(&s.verifier, &100),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn only_the_verifier_or_admin_can_deregister() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 100);
    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.client.try_deregister_verifier(&stranger, &v),
        Err(Ok(Error::Unauthorized))
    );
    s.client.deregister_verifier(&s.admin, &v);
    assert!(!s.client.is_verifier(&v));
}

#[test]
fn deregistering_an_unregistered_address_fails() {
    let s = Setup::new();
    let nobody = Address::generate(&s.env);
    assert_eq!(
        s.client.try_deregister_verifier(&nobody, &nobody),
        Err(Ok(Error::NotVerifier))
    );
}

#[test]
fn withdrawing_without_a_bond_fails() {
    let s = Setup::new();
    let nobody = Address::generate(&s.env);
    assert_eq!(
        s.client.try_withdraw_bond(&nobody),
        Err(Ok(Error::BondNotFound))
    );
}

#[test]
fn an_active_bond_cannot_be_withdrawn() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 100);
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::BondNotWithdrawable))
    );
}

#[test]
fn admin_removal_moves_bond_to_pending_withdrawal() {
    let s = Setup::new();
    s.client.set_bond_withdraw_delay(&(7 * DAY));
    let v = s.bonded_verifier(1_000, 100);

    s.client.remove_verifier(&v);

    assert!(!s.client.is_verifier(&v));
    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.status, BondStatus::PendingWithdrawal);
    assert_eq!(bond.deregistered_at, s.env.ledger().timestamp());
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::WithdrawDelayNotMet))
    );

    let now = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(now + 7 * DAY);
    assert_eq!(s.client.withdraw_bond(&v), 100);
    assert_eq!(s.token.balance(&v), 1_000);
}

#[test]
fn bond_withdrawal_waits_for_the_delay() {
    let s = Setup::new();
    s.client.set_bond_withdraw_delay(&(7 * DAY));
    let v = s.bonded_verifier(1_000, 100);
    s.client.deregister_verifier(&v, &v);

    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::WithdrawDelayNotMet))
    );

    let now = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(now + 7 * DAY - 1);
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::WithdrawDelayNotMet))
    );

    s.env.ledger().set_timestamp(now + 7 * DAY);
    assert_eq!(s.client.withdraw_bond(&v), 100);
    assert_eq!(s.token.balance(&v), 1_000);
    assert_eq!(s.token.balance(&s.client.address), 0);
    assert_eq!(
        s.client.get_verifier_bond(&v).unwrap().status,
        BondStatus::Withdrawn
    );
}

#[test]
fn bond_withdrawal_delay_saturates_when_timestamp_is_less_than_deregistered_at() {
    let s = Setup::new();
    s.client.set_bond_withdraw_delay(&(7 * DAY));
    let v = s.bonded_verifier(1_000, 100);

    let dereg_time = 100_000;
    s.env.ledger().set_timestamp(dereg_time);
    s.client.deregister_verifier(&v, &v);

    // Ledger timestamp equals deregistered_at: elapsed is 0, delay not met
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::WithdrawDelayNotMet))
    );

    // Ledger timestamp goes backward: saturating subtraction evaluates to 0 without underflow panic
    s.env.ledger().set_timestamp(dereg_time - 100);
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::WithdrawDelayNotMet))
    );

    // When time advances past delay: succeeds
    s.env.ledger().set_timestamp(dereg_time + 7 * DAY);
    assert_eq!(s.client.withdraw_bond(&v), 100);
}

#[test]
fn a_bond_cannot_be_withdrawn_twice() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 100);
    s.client.deregister_verifier(&v, &v);
    s.client.withdraw_bond(&v);
    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::BondNotWithdrawable))
    );
}

#[test]
fn verifier_reregistration_preserves_historical_bond_record() {
    let s = Setup::new();
    let v = s.bonded_verifier(2_000, 500);

    // Initial bond is active
    let bond0 = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond0.amount, 500);
    assert_eq!(bond0.status, BondStatus::Active);

    // Cannot re-register while active
    assert_eq!(
        s.client.try_register_with_bond(&v, &600),
        Err(Ok(Error::Unauthorized))
    );

    // Deregister
    s.client.deregister_verifier(&v, &v);
    let bond_pending = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond_pending.status, BondStatus::PendingWithdrawal);

    // Cannot re-register while pending withdrawal
    assert_eq!(
        s.client.try_register_with_bond(&v, &600),
        Err(Ok(Error::Unauthorized))
    );

    // Withdraw bond
    let withdrawn = s.client.withdraw_bond(&v);
    assert_eq!(withdrawn, 500);
    let bond_withdrawn = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond_withdrawn.status, BondStatus::Withdrawn);
    assert_eq!(s.client.get_verifier_bond_nonce(&v), 0);

    // Re-register with new bond amount 600
    s.client.register_with_bond(&v, &600);

    // Archived prior bond preserved
    assert_eq!(s.client.get_verifier_bond_nonce(&v), 1);
    let historical = s.client.get_historical_verifier_bond(&v, &0).unwrap();
    assert_eq!(historical.amount, 500);
    assert_eq!(historical.remaining, 0);
    assert_eq!(historical.status, BondStatus::Withdrawn);

    // New active bond is in place
    let bond1 = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond1.amount, 600);
    assert_eq!(bond1.remaining, 600);
    assert_eq!(bond1.status, BondStatus::Active);
    assert!(s.client.is_verifier(&v));
}

#[test]
fn slashing_moves_the_bond_into_the_recovery_pool_for_donors() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();

    s.client.slash_verifier(&v, &id, &150);

    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.remaining, 250);
    assert_eq!(bond.status, BondStatus::Active);
    assert_eq!(s.client.get_campaign(&id).raised, 0);
    assert_eq!(s.client.slashed_recovery(&id), 150);
}

#[test]
fn slashed_bond_is_split_pro_rata_among_refunding_donors() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign(); // Goal 1000
    let donor_a = s.donor(600);
    let donor_b = s.donor(400);
    s.client.donate(&donor_a, &id, &600);
    s.client.donate(&donor_b, &id, &400);
    s.client.approve_milestone(&id, &s.proof()); // 300 released

    s.client.slash_verifier(&v, &id, &200);
    s.client.cancel_campaign(&s.creator, &id);

    // Pool = (1000 - 300) + 200 = 900, split 60/40.
    assert_eq!(s.client.refund(&donor_a, &id), 540);
    assert_eq!(s.client.refund(&donor_b, &id), 360);
    // Only the unslashed remainder of the bond is left in the contract.
    assert_eq!(s.token.balance(&s.client.address), 200);
}

#[test]
fn slashing_the_whole_bond_marks_it_slashed() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    s.client.slash_verifier(&v, &id, &400);
    assert_eq!(
        s.client.get_verifier_bond(&v).unwrap().status,
        BondStatus::Slashed
    );
}

#[test]
fn slash_validates_its_arguments() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    let nobody = Address::generate(&s.env);

    assert_eq!(
        s.client.try_slash_verifier(&v, &id, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        s.client.try_slash_verifier(&nobody, &id, &10),
        Err(Ok(Error::BondNotFound))
    );
    assert_eq!(
        s.client.try_slash_verifier(&v, &id, &401),
        Err(Ok(Error::SlashExceedsBond))
    );
}

#[test]
fn slash_with_unknown_campaign_fails_and_leaves_bond_untouched() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);

    assert_eq!(
        s.client.try_slash_verifier(&v, &9_999, &150),
        Err(Ok(Error::CampaignNotFound))
    );

    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.remaining, 400);
    assert_eq!(bond.status, BondStatus::Active);
}

#[test]
fn a_withdrawn_bond_cannot_be_slashed() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    s.client.deregister_verifier(&v, &v);
    s.client.withdraw_bond(&v);
    assert_eq!(
        s.client.try_slash_verifier(&v, &id, &10),
        Err(Ok(Error::BondNotWithdrawable))
    );
}

#[test]
fn slashing_completed_campaign_fails_and_does_not_deduct_bond() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    let donor = s.donor(1_000);

    s.client.donate(&donor, &id, &1_000);
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());

    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Completed);

    assert_eq!(
        s.client.try_slash_verifier(&v, &id, &150),
        Err(Ok(Error::CampaignNotActive))
    );

    // Verify bond is not deducted
    let bond = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond.remaining, 400);

    // Slashing a cancelled campaign succeeds
    let id_cancelled = s.campaign();
    s.client.cancel_campaign(&s.creator, &id_cancelled);
    assert_eq!(
        s.client.get_campaign(&id_cancelled).status,
        CampaignStatus::Cancelled
    );
    s.client.slash_verifier(&v, &id_cancelled, &100);
    let bond_after = s.client.get_verifier_bond(&v).unwrap();
    assert_eq!(bond_after.remaining, 300);
    assert_eq!(s.client.slashed_recovery(&id_cancelled), 100);
}

// ─── Sponsor matching pools (issue #27) ───────────────────────────────────────

#[test]
fn pool_config_is_validated() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);

    for (ratio, cap, amount) in [
        (0, 500, 500),
        (10_001, 500, 500),
        (5_000, 0, 0),
        (5_000, 500, 400),
        (5_000, -5, -5),
    ] {
        assert_eq!(
            s.client
                .try_deposit_sponsor_pool(&sponsor, &id, &ratio, &cap, &amount),
            Err(Ok(Error::InvalidMatchingConfig)),
            "ratio {ratio}, cap {cap}, amount {amount}"
        );
    }
    assert_eq!(s.token.balance(&sponsor), 10_000);
}

#[test]
fn pool_needs_an_open_campaign() {
    let s = Setup::new();
    let sponsor = s.donor(10_000);
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &9, &5_000, &500, &500),
        Err(Ok(Error::CampaignNotFound))
    );

    let id = s.campaign();
    s.pass_deadline();
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500),
        Err(Ok(Error::CampaignExpired))
    );
}

#[test]
fn a_sponsor_can_only_open_one_pool_per_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500);
    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500),
        Err(Ok(Error::SponsorPoolExists))
    );
    assert_eq!(s.token.balance(&sponsor), 9_500);
}

#[test]
fn matching_credits_the_campaign_at_the_pool_ratio() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(1_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500);

    s.client.donate(&donor, &id, &200);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &200), 100);

    assert_eq!(s.client.get_campaign(&id).raised, 300);
    let pool = s.client.get_sponsor_pool(&id, &sponsor).unwrap();
    assert_eq!(pool.matched, 100);
    assert_eq!(pool.remaining, 400);
}

#[test]
fn matching_stops_at_the_pool_cap() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(1_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &10_000, &150, &150);

    s.client.donate(&donor, &id, &400);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &400), 150);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &400), 0);
}

#[test]
fn matching_never_pushes_a_campaign_past_its_goal() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(2_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &10_000, &1_000, &1_000);

    s.client.donate(&donor, &id, &900);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &900), 100);
    assert_eq!(s.client.get_campaign(&id).raised, 1_000);
}

#[test]
fn matching_a_tiny_donation_rounds_to_nothing() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(100);
    s.client.deposit_sponsor_pool(&sponsor, &id, &1, &500, &500);
    assert_eq!(s.client.apply_matching(&sponsor, &id, &donor, &1), 0);
}

#[test]
fn matching_validates_amount_and_pool() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(100);

    assert_eq!(
        s.client.try_apply_matching(&sponsor, &id, &donor, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        s.client.try_apply_matching(&sponsor, &id, &donor, &10),
        Err(Ok(Error::SponsorPoolNotFound))
    );
}

#[test]
fn pool_cannot_be_returned_while_the_campaign_is_open() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500);
    assert_eq!(
        s.client.try_return_sponsor_pool(&sponsor, &id),
        Err(Ok(Error::PoolNotReturnable))
    );
}

#[test]
fn unmatched_pool_balance_returns_to_the_sponsor_after_the_deadline() {
    let s = Setup::new();
    let id = s.campaign();
    let sponsor = s.donor(10_000);
    let donor = s.donor(1_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500);
    s.client.donate(&donor, &id, &200);
    s.client.apply_matching(&sponsor, &id, &donor, &200);

    s.pass_deadline();
    assert_eq!(s.client.return_sponsor_pool(&sponsor, &id), 400);
    assert_eq!(s.token.balance(&sponsor), 9_900);
    assert!(s.client.get_sponsor_pool(&id, &sponsor).is_none());
    assert_eq!(
        s.client.try_return_sponsor_pool(&sponsor, &id),
        Err(Ok(Error::SponsorPoolNotFound))
    );
}

// ─── Pledges (issue #30) ──────────────────────────────────────────────────────

#[test]
fn pledge_amount_must_be_positive() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    assert_eq!(
        s.client.try_create_pledge(&donor, &id, &0),
        Err(Ok(Error::InvalidPledge))
    );
    assert_eq!(
        s.client.try_create_pledge(&donor, &id, &-5),
        Err(Ok(Error::InvalidPledge))
    );
}

#[test]
fn pledge_needs_enough_allowance() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    assert_eq!(
        s.client.try_create_pledge(&donor, &id, &300),
        Err(Ok(Error::InvalidPledge))
    );

    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&donor, &s.client.address, &299, &expiry);
    assert_eq!(
        s.client.try_create_pledge(&donor, &id, &300),
        Err(Ok(Error::InvalidPledge))
    );
}

#[test]
fn pledge_needs_an_open_campaign() {
    let s = Setup::new();
    let donor = s.donor(1_000);
    assert_eq!(
        s.client.try_create_pledge(&donor, &7, &100),
        Err(Ok(Error::CampaignNotFound))
    );
    let id = s.campaign();
    s.pass_deadline();
    assert_eq!(
        s.client.try_create_pledge(&donor, &id, &100),
        Err(Ok(Error::CampaignExpired))
    );
}

#[test]
fn a_pledge_moves_no_money_until_a_milestone_is_approved() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&donor, &s.client.address, &300, &expiry);

    let pledge_id = s.client.create_pledge(&donor, &id, &300);
    assert_eq!(s.token.balance(&donor), 1_000);
    assert_eq!(s.client.get_campaign(&id).raised, 0);
    let pledge = s.client.get_pledge(&pledge_id).unwrap();
    assert_eq!(pledge.pledged_amount, 300);
    assert_eq!(pledge.pulled_amount, 0);
    assert!(pledge.active);

    s.client.approve_milestone(&id, &s.proof());

    assert_eq!(s.token.balance(&donor), 700);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert_eq!(s.client.get_pledge(&pledge_id).unwrap().pulled_amount, 300);
}

#[test]
fn donor_can_cancel_pledge() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&donor, &s.client.address, &300, &expiry);

    let pledge_id = s.client.create_pledge(&donor, &id, &300);
    assert!(s.client.get_pledge(&pledge_id).unwrap().active);

    s.client.cancel_pledge(&donor, &pledge_id);
    assert!(!s.env.events().all().events().is_empty());

    let pledge = s.client.get_pledge(&pledge_id).unwrap();
    assert!(!pledge.active);
}

#[test]
fn cancelled_pledge_is_never_pulled() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    let other_donor = s.donor(1_000);
    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&donor, &s.client.address, &300, &expiry);

    let pledge_id = s.client.create_pledge(&donor, &id, &300);
    s.client.cancel_pledge(&donor, &pledge_id);

    // Direct donation covers milestone 0 (300)
    s.client.donate(&other_donor, &id, &300);

    // Milestone approval pulls from active pledges and escrow
    s.client.approve_milestone(&id, &s.proof());

    // Donor balance was NOT pulled
    assert_eq!(s.token.balance(&donor), 1_000);
    let pledge = s.client.get_pledge(&pledge_id).unwrap();
    assert_eq!(pledge.pulled_amount, 0);
    assert!(!pledge.active);
}

#[test]
fn cancelling_pledge_twice_fails() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&donor, &s.client.address, &300, &expiry);

    let pledge_id = s.client.create_pledge(&donor, &id, &300);
    s.client.cancel_pledge(&donor, &pledge_id);

    let res = s.client.try_cancel_pledge(&donor, &pledge_id);
    assert_eq!(res, Err(Ok(Error::PledgeAlreadyCancelled)));
}

#[test]
fn only_pledge_donor_can_cancel() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    let stranger = Address::generate(&s.env);
    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&donor, &s.client.address, &300, &expiry);

    let pledge_id = s.client.create_pledge(&donor, &id, &300);

    // Stranger calls with stranger's auth
    let auth = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "cancel_pledge",
            args: (stranger.clone(), pledge_id).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res = s
        .client
        .mock_auths(&[auth])
        .try_cancel_pledge(&stranger, &pledge_id);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));

    // Stranger pretending to pass donor address without donor's auth
    let auth_fake = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "cancel_pledge",
            args: (donor.clone(), pledge_id).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res_fake = s
        .client
        .mock_auths(&[auth_fake])
        .try_cancel_pledge(&donor, &pledge_id);
    assert_eq!(res_fake.unwrap_err(), Err(InvokeError::Abort));

    // Pledge remains active
    assert!(s.client.get_pledge(&pledge_id).unwrap().active);
}

#[test]
fn cancel_nonexistent_pledge_fails() {
    let s = Setup::new();
    let donor = Address::generate(&s.env);
    let res = s.client.try_cancel_pledge(&donor, &999_999);
    assert_eq!(res, Err(Ok(Error::InvalidPledge)));
}

#[test]
fn extend_campaign_ttl_allows_entry_to_survive_past_original_ttl() {
    let s = Setup::new();
    let id = s.campaign();

    // Verify initial campaign exists
    let initial = s.client.get_campaign(&id);
    assert_eq!(initial.id, id);

    // 120 days in ledgers = 120 * 17_280 = 2_073_600
    let day_in_ledgers = 17_280;

    // Advance sequence number to day 100
    s.env.ledger().set_sequence_number(100 * day_in_ledgers);

    // Extend TTL publicly without any auth
    s.client.extend_campaign_ttl(&id);

    // Advance past the original 120-day persistent lifetime (to day 150)
    s.env.ledger().set_sequence_number(150 * day_in_ledgers);

    // The campaign entry survives because its TTL was extended
    let campaign = s.client.get_campaign(&id);
    assert_eq!(campaign.id, id);
    assert_eq!(campaign.goal, 1000);
}

#[test]
fn extend_campaign_ttl_validates_existence_and_requires_no_auth() {
    let s = Setup::new();
    let id = s.campaign();

    // Non-existent campaign returns CampaignNotFound
    assert_eq!(
        s.client.try_extend_campaign_ttl(&9999),
        Err(Ok(Error::CampaignNotFound))
    );

    // Any caller can extend TTL without specific authorization
    assert_eq!(s.client.try_extend_campaign_ttl(&id), Ok(Ok(())));
}

// ─── Issue #9 Tests: Multisig verification ───────────────────────────────────

#[test]
fn multisig_campaign_creation_validation() {
    let s = Setup::new();
    let v1 = s.verifier.clone();
    let v2 = Address::generate(&s.env);
    let v3 = Address::generate(&s.env);
    s.client.add_verifier(&v2);
    s.client.add_verifier(&v3);

    let verifiers = vec![&s.env, v1.clone(), v2.clone(), v3.clone()];
    let milestones = vec![&s.env, 500, 500];
    let deadline = s.env.ledger().timestamp() + 30 * DAY;
    let uri = String::from_str(&s.env, "ipfs://multisig-trees");

    // Valid 2 of 3 creation
    let id = s.client.create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    let c = s.client.get_campaign(&id);
    assert_eq!(c.threshold, 2);
    assert_eq!(c.verifiers.len(), 3);
    assert_eq!(c.goal, 1000);

    // Invalid threshold: 0
    let err_zero = s.client.try_create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &0,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    assert_eq!(err_zero, Err(Ok(Error::InvalidThreshold)));

    // Invalid threshold: exceeds verifiers count (4 > 3)
    let err_exceed = s.client.try_create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &4,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    assert_eq!(err_exceed, Err(Ok(Error::InvalidThreshold)));

    // Empty verifiers list
    let empty_verifiers = vec![&s.env];
    let err_empty = s.client.try_create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &empty_verifiers,
        &1,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    assert_eq!(err_empty, Err(Ok(Error::InvalidThreshold)));

    // Duplicate verifiers
    let dup_verifiers = vec![&s.env, v1.clone(), v2.clone(), v1.clone()];
    let err_dup = s.client.try_create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &dup_verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    assert_eq!(err_dup, Err(Ok(Error::DuplicateVerifier)));

    // Unregistered verifier in list
    let stranger = Address::generate(&s.env);
    let unreg_verifiers = vec![&s.env, v1, v2, stranger];
    let err_unreg = s.client.try_create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &unreg_verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    assert_eq!(err_unreg, Err(Ok(Error::NotVerifier)));
}

#[test]
fn multisig_milestone_release_requires_threshold() {
    let s = Setup::new();
    let v1 = s.verifier.clone();
    let v2 = Address::generate(&s.env);
    let v3 = Address::generate(&s.env);
    s.client.add_verifier(&v2);
    s.client.add_verifier(&v3);

    let verifiers = vec![&s.env, v1.clone(), v2.clone(), v3.clone()];
    let milestones = vec![&s.env, 300, 700];
    let deadline = s.env.ledger().timestamp() + 30 * DAY;
    let uri = String::from_str(&s.env, "ipfs://multisig-trees");

    let id = s.client.create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );

    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // Single verifier approval (1 of 2 threshold) -> returns 0, no funds released yet
    let first_res = s.client.approve_milestone_multisig(&id, &v1, &s.proof());
    assert_eq!(first_res, 0);
    assert_eq!(s.token.balance(&s.beneficiary), 0);
    let c = s.client.get_campaign(&id);
    assert_eq!(c.milestones_released, 0);
    assert_eq!(c.released, 0);

    // Approvals recorded for milestone 0
    let approvals = s.client.get_milestone_approvals(&id, &0);
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals.get(0).unwrap(), v1);

    // Duplicate approval by same verifier (v1) -> rejected
    let dup_res = s
        .client
        .try_approve_milestone_multisig(&id, &v1, &s.proof());
    assert_eq!(dup_res, Err(Ok(Error::DuplicateApproval)));

    // Approval by non-assigned verifier -> rejected
    let v4 = Address::generate(&s.env);
    s.client.add_verifier(&v4);
    let non_assigned = s
        .client
        .try_approve_milestone_multisig(&id, &v4, &s.proof());
    assert_eq!(non_assigned, Err(Ok(Error::NotVerifier)));

    // Second approval by v2 reaches threshold (2 of 2) -> releases milestone 0 (300)
    let second_res = s.client.approve_milestone_multisig(&id, &v2, &s.proof());
    assert_eq!(second_res, 300);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    let c_after = s.client.get_campaign(&id);
    assert_eq!(c_after.milestones_released, 1);
    assert_eq!(c_after.released, 300);

    // Second milestone: v3 approves first (1 of 2) -> returns 0
    let m2_first = s.client.approve_milestone_multisig(&id, &v3, &s.proof());
    assert_eq!(m2_first, 0);
    assert_eq!(s.token.balance(&s.beneficiary), 300);

    // v1 approves second (2 of 2) -> releases milestone 1 (700)
    let m2_second = s.client.approve_milestone_multisig(&id, &v1, &s.proof());
    assert_eq!(m2_second, 700);
    assert_eq!(s.token.balance(&s.beneficiary), 1000);
    let c_final = s.client.get_campaign(&id);
    assert_eq!(c_final.status, CampaignStatus::Completed);
}

#[test]
fn single_verifier_campaign_works_unchanged() {
    let s = Setup::new();
    let id = s.campaign(); // 3 milestones: 300, 300, 400
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // approve_milestone with original 2-arg signature works for single verifier campaign
    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 300);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
}

#[test]
fn multisig_campaign_blocks_anonymous_approve_milestone() {
    let s = Setup::new();
    let v1 = s.verifier.clone();
    let v2 = Address::generate(&s.env);
    s.client.add_verifier(&v2);

    let verifiers = vec![&s.env, v1, v2];
    let milestones = vec![&s.env, 500];
    let deadline = s.env.ledger().timestamp() + 30 * DAY;
    let uri = String::from_str(&s.env, "ipfs://trees");

    let id = s.client.create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    let donor = s.donor(500);
    s.client.donate(&donor, &id, &500);

    // Calling approve_milestone without specifying verifier on a multisig campaign returns Unauthorized
    let res = s.client.try_approve_milestone(&id, &s.proof());
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
}

// ─── Issue #10 Tests: Per-campaign tokens and allowlist ──────────────────────

#[test]
fn admin_manages_token_allowlist() {
    let s = Setup::new();
    let new_token = Address::generate(&s.env);

    // Initially not allowed
    assert!(!s.client.is_token_allowed(&new_token));

    // Admin adds token
    s.client.add_allowed_token(&new_token);
    assert!(s.client.is_token_allowed(&new_token));

    // Admin removes token
    s.client.remove_allowed_token(&new_token);
    assert!(!s.client.is_token_allowed(&new_token));
}

#[test]
fn unallowed_token_campaign_creation_fails() {
    let s = Setup::new();
    let unallowed_tok = Address::generate(&s.env);

    let res = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &unallowed_tok,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://fail"),
        &(s.env.ledger().timestamp() + DAY),
        &vec![&s.env, 100],
    );
    assert_eq!(res, Err(Ok(Error::TokenNotAllowed)));
}

#[test]
fn campaigns_with_different_tokens_operate_side_by_side() {
    let s = Setup::new();
    let env = &s.env;

    // Create a second token: USDC SAC
    let usdc_sac = env.register_stellar_asset_contract_v2(s.admin.clone());
    let usdc = TokenClient::new(env, &usdc_sac.address());
    let usdc_addr = usdc_sac.address();

    // Admin adds USDC to allowlist
    s.client.add_allowed_token(&usdc_addr);
    assert!(s.client.is_token_allowed(&usdc_addr));

    // Campaign 1: raises in default token
    let c1_id = s.campaign();

    // Campaign 2: raises in USDC
    let c2_id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &usdc_addr,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://usdc-campaign"),
        &(env.ledger().timestamp() + 30 * DAY),
        &vec![env, 400, 600],
    );

    let c1 = s.client.get_campaign(&c1_id);
    let c2 = s.client.get_campaign(&c2_id);
    assert_eq!(c1.token, s.token.address);
    assert_eq!(c2.token, usdc_addr);

    // Donors: donor1 has token A, donor2 has USDC
    let donor1 = s.donor(1000);
    let donor2 = Address::generate(env);
    StellarAssetClient::new(env, &usdc_addr).mint(&donor2, &1000);

    // Donate to Campaign 1 with token A
    s.client.donate(&donor1, &c1_id, &500);
    // Donate to Campaign 2 with USDC
    s.client.donate(&donor2, &c2_id, &500);

    // Check balances of escrow contract
    assert_eq!(s.token.balance(&s.client.address), 500);
    assert_eq!(usdc.balance(&s.client.address), 500);
    assert_eq!(s.token.balance(&s.beneficiary), 0);
    assert_eq!(usdc.balance(&s.beneficiary), 0);

    // Approve milestone on Campaign 1 (releases 300 in token A)
    s.client.approve_milestone(&c1_id, &s.proof());
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert_eq!(usdc.balance(&s.beneficiary), 0);
    assert_eq!(s.token.balance(&s.client.address), 200);

    // Approve milestone on Campaign 2 (releases 400 in USDC)
    s.client.approve_milestone(&c2_id, &s.proof());
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert_eq!(usdc.balance(&s.beneficiary), 400);
    assert_eq!(usdc.balance(&s.client.address), 100);

    // Cancel Campaign 2 and test refund in USDC
    s.client.cancel_campaign(&s.creator, &c2_id);
    let refund_amt = s.client.refund(&donor2, &c2_id);
    assert_eq!(refund_amt, 100);
    assert_eq!(usdc.balance(&donor2), 600);
    assert_eq!(usdc.balance(&s.client.address), 0);
}

// =============================================================================
// Read-only view function tests
// =============================================================================

#[test]
fn test_view_campaign_count() {
    let s = Setup::new();
    assert_eq!(s.client.campaign_count(), 0);

    let c1 = s.campaign();
    assert_eq!(c1, 0);
    assert_eq!(s.client.campaign_count(), 1);

    let c2 = s.campaign();
    assert_eq!(c2, 1);
    assert_eq!(s.client.campaign_count(), 2);
}

#[test]
fn test_view_contribution_of() {
    let s = Setup::new();
    let cid = s.campaign();
    let donor = s.donor(1000);

    // Initial contribution is 0
    assert_eq!(s.client.contribution_of(&cid, &donor), 0);

    // First donation
    s.client.donate(&donor, &cid, &300);
    assert_eq!(s.client.contribution_of(&cid, &donor), 300);

    // Cumulative donation
    s.client.donate(&donor, &cid, &200);
    assert_eq!(s.client.contribution_of(&cid, &donor), 500);

    // Cancel campaign and refund donor -> contribution resets to 0
    s.client.cancel_campaign(&s.creator, &cid);
    s.client.refund(&donor, &cid);
    assert_eq!(s.client.contribution_of(&cid, &donor), 0);
}

#[test]
fn test_view_refundable_amount() {
    let s = Setup::new();
    let id = s.campaign();
    let donor1 = s.donor(1000);
    let donor2 = s.donor(1000);
    let stranger = Address::generate(&s.env);

    // 1. Non-existent campaign returns 0
    assert_eq!(s.client.refundable_amount(&999_999, &donor1), 0);

    // 2. Active campaign within deadline is not refundable -> returns 0
    s.client.donate(&donor1, &id, &500);
    s.client.donate(&donor2, &id, &500);
    assert_eq!(s.client.refundable_amount(&id, &donor1), 0);
    assert_eq!(s.client.refundable_amount(&id, &stranger), 0);

    // 3. Cancelled campaign matches refund amount
    // Release milestone 0 (300). Raised: 1000, Released: 300, Unreleased: 700.
    s.client.approve_milestone(&id, &s.proof());
    let cancel_id = id;
    s.client.cancel_campaign(&s.creator, &cancel_id);

    // donor1 contributed 500: 500 * 700 / 1000 = 350
    assert_eq!(s.client.refundable_amount(&cancel_id, &donor1), 350);
    assert_eq!(s.client.refundable_amount(&cancel_id, &donor2), 350);
    assert_eq!(s.client.refundable_amount(&cancel_id, &stranger), 0);

    // Calling refund pays exactly what refundable_amount reported
    let refunded = s.client.refund(&donor1, &cancel_id);
    assert_eq!(refunded, 350);
    // After refund, donor1 has no recorded contribution -> refundable_amount is 0
    assert_eq!(s.client.refundable_amount(&cancel_id, &donor1), 0);
    // donor2 still has 350 refundable
    assert_eq!(s.client.refundable_amount(&cancel_id, &donor2), 350);

    // 4. Expired campaign matches refund amount
    let s_exp = Setup::new();
    let exp_id = s_exp.campaign();
    let d = s_exp.donor(600);
    s_exp.client.donate(&d, &exp_id, &600);
    assert_eq!(s_exp.client.refundable_amount(&exp_id, &d), 0);

    s_exp.pass_deadline();
    // 600 * 1000 / 600 = 600 (full refund)
    assert_eq!(s_exp.client.refundable_amount(&exp_id, &d), 600);
    assert_eq!(s_exp.client.refund(&d, &exp_id), 600);
    assert_eq!(s_exp.client.refundable_amount(&exp_id, &d), 0);

    // 5. Overdue milestone in active campaign matches refund amount
    let s_overdue = Setup::new();
    let now = s_overdue.env.ledger().timestamp();
    let due_dates = vec![&s_overdue.env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![&s_overdue.env, 300, 700];
    let overdue_id = s_overdue.client.create_campaign_with_due_dates(
        &s_overdue.creator,
        &s_overdue.beneficiary,
        &s_overdue.verifier,
        &s_overdue.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s_overdue.env, "ipfs://overdue-view"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );
    let d_overdue = s_overdue.donor(500);
    s_overdue.client.donate(&d_overdue, &overdue_id, &500);
    assert_eq!(
        s_overdue.client.refundable_amount(&overdue_id, &d_overdue),
        0
    );

    s_overdue.env.ledger().set_timestamp(now + 11 * DAY);
    assert_eq!(
        s_overdue.client.refundable_amount(&overdue_id, &d_overdue),
        500
    );
    assert_eq!(s_overdue.client.refund(&d_overdue, &overdue_id), 500);
    assert_eq!(
        s_overdue.client.refundable_amount(&overdue_id, &d_overdue),
        0
    );

    // 6. Completed campaign returns 0
    let s_comp = Setup::new();
    let comp_id = s_comp.campaign();
    let d_comp = s_comp.donor(1000);
    s_comp.client.donate(&d_comp, &comp_id, &1000);
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    assert_eq!(s_comp.client.refundable_amount(&comp_id, &d_comp), 0);
}

#[test]
fn test_view_effective_status() {
    let s = Setup::new();

    // 1. Unknown campaign returns CampaignNotFound
    assert_eq!(
        s.client.try_effective_status(&999_999),
        Err(Ok(Error::CampaignNotFound))
    );

    // 2. Active campaign within deadline -> Active
    let id = s.campaign();
    assert_eq!(s.client.effective_status(&id), CampaignStatus::Active);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Active);

    // 3. Active campaign past deadline -> Expired
    s.pass_deadline();
    assert_eq!(s.client.effective_status(&id), CampaignStatus::Expired);
    // Note: stored status remains Active as per ADR-0002
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Active);

    // 4. Cancelled campaign -> Cancelled (before and after deadline)
    let s_cancel = Setup::new();
    let cancel_id = s_cancel.campaign();
    s_cancel
        .client
        .cancel_campaign(&s_cancel.creator, &cancel_id);
    assert_eq!(
        s_cancel.client.effective_status(&cancel_id),
        CampaignStatus::Cancelled
    );
    s_cancel.pass_deadline();
    assert_eq!(
        s_cancel.client.effective_status(&cancel_id),
        CampaignStatus::Cancelled
    );

    // 5. Completed campaign -> Completed (before and after deadline)
    let s_comp = Setup::new();
    let comp_id = s_comp.campaign();
    let d = s_comp.donor(1000);
    s_comp.client.donate(&d, &comp_id, &1000);
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    assert_eq!(
        s_comp.client.effective_status(&comp_id),
        CampaignStatus::Completed
    );
    s_comp.pass_deadline();
    assert_eq!(
        s_comp.client.effective_status(&comp_id),
        CampaignStatus::Completed
    );
}

#[test]
fn test_view_milestone_due_dates() {
    let s = Setup::new();

    // 1. Unknown campaign returns CampaignNotFound
    assert_eq!(
        s.client.try_milestone_due_dates(&999_999),
        Err(Ok(Error::CampaignNotFound))
    );

    // 2. Campaign created with create_campaign returns empty vector
    let id_standard = s.campaign();
    let due_dates_empty = s.client.milestone_due_dates(&id_standard);
    assert_eq!(due_dates_empty.len(), 0);

    // 3. Campaign created with create_campaign_with_due_dates returns stored due dates
    let now = s.env.ledger().timestamp();
    let expected_dates = vec![&s.env, now + 5 * DAY, now + 15 * DAY, now + 25 * DAY];
    let milestones = vec![&s.env, 300, 300, 400];
    let id_with_dates = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://due-dates-view"),
        &(now + 30 * DAY),
        &milestones,
        &expected_dates,
    );

    let retrieved_dates = s.client.milestone_due_dates(&id_with_dates);
    assert_eq!(retrieved_dates, expected_dates);
}

#[test]
fn create_campaign_with_due_dates_validates_future_and_monotonicity() {
    let s = Setup::new();
    let now = s.env.ledger().timestamp();
    let milestones = vec![&s.env, 300, 700];

    let count_before = s.client.campaign_count();

    // 1. Due date in the past fails with DeadlineInPast
    let past_dates = vec![&s.env, now - 10, now + 100];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://past-due"),
            &(now + 30 * DAY),
            &milestones,
            &past_dates,
        ),
        Err(Ok(Error::DeadlineInPast))
    );

    // 2. Due date equal to current timestamp fails with DeadlineInPast
    let now_dates = vec![&s.env, now, now + 100];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://now-due"),
            &(now + 30 * DAY),
            &milestones,
            &now_dates,
        ),
        Err(Ok(Error::DeadlineInPast))
    );

    // 3. Non-monotonic due dates fail with InvalidMilestones (decreasing)
    let decreasing_dates = vec![&s.env, now + 100, now + 50];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://dec-due"),
            &(now + 30 * DAY),
            &milestones,
            &decreasing_dates,
        ),
        Err(Ok(Error::InvalidMilestones))
    );

    // 4. Equal consecutive due dates fail with InvalidMilestones
    let equal_dates = vec![&s.env, now + 100, now + 100];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://equal-due"),
            &(now + 30 * DAY),
            &milestones,
            &equal_dates,
        ),
        Err(Ok(Error::InvalidMilestones))
    );

    // Verify campaign was not created on validation failures
    assert_eq!(s.client.campaign_count(), count_before);

    // 5. Valid future monotonic due dates succeed
    let valid_dates = vec![&s.env, now + 100, now + 200];
    let res = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://valid-due"),
        &(now + 30 * DAY),
        &milestones,
        &valid_dates,
    );
    assert_eq!(s.client.campaign_count(), count_before + 1);
    assert_eq!(s.client.milestone_due_dates(&res), valid_dates);
}

#[test]
fn test_view_is_verifier() {
    let s = Setup::new();
    let candidate = Address::generate(&s.env);

    // Initially not a verifier
    assert!(!s.client.is_verifier(&candidate));

    // Admin adds verifier
    s.client.add_verifier(&candidate);
    assert!(s.client.is_verifier(&candidate));

    // Admin removes verifier
    s.client.remove_verifier(&candidate);
    assert!(!s.client.is_verifier(&candidate));
}

#[test]
fn test_view_admin() {
    let s = Setup::new();
    assert_eq!(s.client.admin(), s.admin);

    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);
    assert_eq!(s.client.admin(), new_admin);
}

#[test]
fn test_view_token() {
    let s = Setup::new();
    assert_eq!(s.client.token(), s.token.address);
}

#[test]
fn test_view_version() {
    let s = Setup::new();
    assert_eq!(
        s.client.version(),
        String::from_str(&s.env, env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(s.client.version(), String::from_str(&s.env, "0.1.0"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_refund_invariants_hold_after_cancellation(
        milestone_amounts in prop::collection::vec(100i128..10_000i128, 1..=5),
        donor_donations in prop::collection::vec(10i128..5_000i128, 1..=8),
        num_milestones_to_release in 0usize..5,
    ) {
        let s = Setup::new();
        let mut sdk_milestones = soroban_sdk::Vec::new(&s.env);
        let mut goal: i128 = 0;
        for &m in milestone_amounts.iter() {
            sdk_milestones.push_back(m);
            goal += m;
        }

        let deadline = s.env.ledger().timestamp() + 30 * DAY;
        let id = s.client.create_campaign(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Climate,
            &String::from_str(&s.env, "ipfs://prop-cancel"),
            &deadline,
            &sdk_milestones,
        );

        let mut total_raised: i128 = 0;
        let mut active_donors = soroban_sdk::Vec::new(&s.env);

        for &amount in donor_donations.iter() {
            if total_raised >= goal {
                break;
            }
            let donor = s.donor(amount);
            let accepted = s.client.donate(&donor, &id, &amount);
            if accepted > 0 {
                total_raised += accepted;
                active_donors.push_back(donor);
            }
        }

        let mut cumulative: i128 = 0;
        let mut max_releasable: usize = 0;
        for &m in milestone_amounts.iter() {
            if cumulative + m <= total_raised {
                cumulative += m;
                max_releasable += 1;
            } else {
                break;
            }
        }

        let release_count = num_milestones_to_release.min(max_releasable);
        let mut total_released: i128 = 0;
        for _ in 0..release_count {
            let released = s.client.approve_milestone(&id, &s.proof());
            total_released += released;
        }

        if release_count < milestone_amounts.len() {
            // Campaign remains Active; cancellation succeeds and opens pro-rata refunds.
            s.client.cancel_campaign(&s.creator, &id);

            let mut total_refunded: i128 = 0;
            for donor in active_donors.iter() {
                let refund = s.client.refund(&donor, &id);
                prop_assert!(refund >= 0, "Refund must be non-negative");
                total_refunded += refund;
            }

            // 1. Total refunds plus releases never exceed total raised
            prop_assert!(
                total_refunded + total_released <= total_raised,
                "Total refunded ({}) + total released ({}) exceeded total raised ({})",
                total_refunded, total_released, total_raised
            );

            // 2. Contract balance never goes negative and equals unreleased dust
            let contract_balance = s.token.balance(&s.client.address);
            prop_assert!(
                contract_balance >= 0,
                "Contract balance is negative: {}",
                contract_balance
            );
            prop_assert_eq!(
                contract_balance,
                total_raised - total_released - total_refunded,
                "Contract balance did not match total_raised - total_released - total_refunded"
            );
            prop_assert_eq!(s.token.balance(&s.beneficiary), total_released);
        } else {
            // All milestones were approved; campaign is Completed and funds delivered.
            let c = s.client.get_campaign(&id);
            prop_assert_eq!(c.status, CampaignStatus::Completed);
            prop_assert!(s.client.try_cancel_campaign(&s.creator, &id).is_err());
            for donor in active_donors.iter() {
                prop_assert!(s.client.try_refund(&donor, &id).is_err());
            }
            prop_assert_eq!(s.token.balance(&s.beneficiary), total_released);
            let contract_balance = s.token.balance(&s.client.address);
            prop_assert!(contract_balance >= 0);
            prop_assert_eq!(contract_balance, total_raised - total_released);
        }
    }

    #[test]
    fn prop_refund_invariants_hold_after_expiry(
        milestone_amounts in prop::collection::vec(100i128..10_000i128, 1..=5),
        donor_donations in prop::collection::vec(10i128..5_000i128, 1..=8),
        num_milestones_to_release in 0usize..5,
    ) {
        let s = Setup::new();
        let mut sdk_milestones = soroban_sdk::Vec::new(&s.env);
        let mut goal: i128 = 0;
        for &m in milestone_amounts.iter() {
            sdk_milestones.push_back(m);
            goal += m;
        }

        let deadline = s.env.ledger().timestamp() + 30 * DAY;
        let id = s.client.create_campaign(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Climate,
            &String::from_str(&s.env, "ipfs://prop-expiry"),
            &deadline,
            &sdk_milestones,
        );

        let mut total_raised: i128 = 0;
        let mut active_donors = soroban_sdk::Vec::new(&s.env);

        for &amount in donor_donations.iter() {
            if total_raised >= goal {
                break;
            }
            let donor = s.donor(amount);
            let accepted = s.client.donate(&donor, &id, &amount);
            if accepted > 0 {
                total_raised += accepted;
                active_donors.push_back(donor);
            }
        }

        let mut cumulative: i128 = 0;
        let mut max_releasable: usize = 0;
        for &m in milestone_amounts.iter() {
            if cumulative + m <= total_raised {
                cumulative += m;
                max_releasable += 1;
            } else {
                break;
            }
        }

        let release_count = num_milestones_to_release.min(max_releasable);
        let mut total_released: i128 = 0;
        for _ in 0..release_count {
            let released = s.client.approve_milestone(&id, &s.proof());
            total_released += released;
        }

        // Pass deadline to trigger expiry refunds
        s.env.ledger().set_timestamp(deadline + 1);

        if release_count < milestone_amounts.len() {
            let mut total_refunded: i128 = 0;
            for donor in active_donors.iter() {
                let refund = s.client.refund(&donor, &id);
                prop_assert!(refund >= 0, "Refund must be non-negative");
                total_refunded += refund;
            }

            // Invariants
            prop_assert!(
                total_refunded + total_released <= total_raised,
                "Total refunded ({}) + total released ({}) exceeded total raised ({})",
                total_refunded, total_released, total_raised
            );
            let contract_balance = s.token.balance(&s.client.address);
            prop_assert!(contract_balance >= 0, "Negative balance");
            prop_assert_eq!(
                contract_balance,
                total_raised - total_released - total_refunded
            );
        } else {
            // Already completed
            let c = s.client.get_campaign(&id);
            prop_assert_eq!(c.status, CampaignStatus::Completed);
            for donor in active_donors.iter() {
                prop_assert!(s.client.try_refund(&donor, &id).is_err());
            }
            let contract_balance = s.token.balance(&s.client.address);
            prop_assert!(contract_balance >= 0);
            prop_assert_eq!(contract_balance, total_raised - total_released);
        }
    }
}

// ─── Emergency pause switch tests ────────────────────────────────────────────

#[test]
fn admin_can_pause_and_unpause() {
    let s = Setup::new();
    assert!(!s.client.is_paused());

    s.client.pause();
    assert!(s.client.is_paused());

    s.client.unpause();
    assert!(!s.client.is_paused());
}

#[test]
fn pause_blocks_create_campaign_donate_and_approve_milestone() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);

    // Initial donation works
    let accepted = s.client.donate(&donor, &id, &300);
    assert_eq!(accepted, 300);

    // Pause contract
    s.client.pause();

    // Create campaign blocked
    let res_create = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://new-campaign"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 100],
    );
    assert_eq!(res_create, Err(Ok(Error::Paused)));

    // Create campaign with due dates blocked
    let res_create_dates = s.client.try_create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://new-dates"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 100],
        &vec![&s.env, s.env.ledger().timestamp() + 10 * DAY],
    );
    assert_eq!(res_create_dates, Err(Ok(Error::Paused)));

    // Donate blocked
    let res_donate = s.client.try_donate(&donor, &id, &100);
    assert_eq!(res_donate, Err(Ok(Error::Paused)));

    // Approve milestone blocked
    let res_approve = s.client.try_approve_milestone(&id, &s.proof());
    assert_eq!(res_approve, Err(Ok(Error::Paused)));

    // Emergency fast track blocked
    let res_fast_track = s.client.try_emergency_fast_track(&id);
    assert_eq!(res_fast_track, Err(Ok(Error::Paused)));

    // Unpause restores functionality
    s.client.unpause();

    let accepted2 = s.client.donate(&donor, &id, &100);
    assert_eq!(accepted2, 100);

    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 300);
}

#[test]
fn refunds_work_while_paused() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);
    s.client.donate(&donor, &id, &300);

    s.client.cancel_campaign(&s.creator, &id);

    // Pause contract
    s.client.pause();

    // Refund succeeds while paused
    let refunded = s.client.refund(&donor, &id);
    assert_eq!(refunded, 300);
    assert_eq!(s.token.balance(&donor), 500);
}

// ─── Upgrade tests ────────────────────────────────────────────────────────────

#[contract]
pub struct MockUpgradedAidline;

#[contractimpl]
impl MockUpgradedAidline {
    pub fn version() -> u32 {
        2
    }

    pub fn get_campaign(env: Env, campaign_id: u64) -> crate::Campaign {
        crate::storage::campaign(&env, campaign_id).unwrap()
    }
}

#[test]
fn admin_can_upgrade_contract_and_preserves_state() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);
    s.client.donate(&donor, &id, &300);

    // Upload the new contract code
    let new_wasm_hash = s.env.upload(MockUpgradedAidline);

    // Admin upgrades the contract
    s.client.upgrade(&new_wasm_hash);

    // Verify upgrade took effect at the same contract address
    let upgraded_client = MockUpgradedAidlineClient::new(&s.env, &s.client.address);
    assert_eq!(upgraded_client.version(), 2);

    // State (campaign) is preserved across upgrade
    let c = upgraded_client.get_campaign(&id);
    assert_eq!(c.id, id);
    assert_eq!(c.goal, 1000);
    assert_eq!(c.raised, 300);
}

#[test]
fn non_admin_cannot_upgrade_contract() {
    let s = Setup::new();
    let new_wasm_hash = s.env.upload(MockUpgradedAidline);

    let res = s.client.mock_auths(&[]).try_upgrade(&new_wasm_hash);
    assert!(res.is_err());
}

// ─── Metadata update tests ───────────────────────────────────────────────────

#[test]
fn creator_can_update_metadata_before_donations() {
    let s = Setup::new();
    let id = s.campaign();

    let initial = s.client.get_campaign(&id);
    assert_eq!(
        initial.metadata_uri,
        String::from_str(&s.env, "ipfs://flood-relief")
    );

    let new_uri = String::from_str(&s.env, "ipfs://flood-relief-corrected-story");
    s.client.update_metadata(&id, &new_uri);

    let updated = s.client.get_campaign(&id);
    assert_eq!(updated.metadata_uri, new_uri);
}

#[test]
fn metadata_update_fails_after_first_donation() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);

    // First donation
    s.client.donate(&donor, &id, &100);

    let new_uri = String::from_str(&s.env, "ipfs://too-late-story");
    let res = s.client.try_update_metadata(&id, &new_uri);
    assert_eq!(res, Err(Ok(Error::DonationsAlreadyStarted)));
}

#[test]
fn metadata_update_requires_creator_auth() {
    let s = Setup::new();
    let id = s.campaign();
    let new_uri = String::from_str(&s.env, "ipfs://new-story");

    // Call without creator authorization
    let res = s.client.mock_auths(&[]).try_update_metadata(&id, &new_uri);
    assert!(res.is_err());
}

#[test]
fn metadata_update_fails_for_expired_or_nonexistent_campaign() {
    let s = Setup::new();
    let uri = String::from_str(&s.env, "ipfs://test");

    // Nonexistent
    assert_eq!(
        s.client.try_update_metadata(&999, &uri),
        Err(Ok(Error::CampaignNotFound))
    );

    // Expired
    let id = s.campaign();
    s.pass_deadline();
    assert_eq!(
        s.client.try_update_metadata(&id, &uri),
        Err(Ok(Error::CampaignExpired))
    );
}

// ─── Deadline extension tests ────────────────────────────────────────────────

#[test]
fn creator_can_extend_deadline_once() {
    let s = Setup::new();
    let id = s.campaign();

    let initial = s.client.get_campaign(&id);
    let original_deadline = initial.deadline;
    assert!(!initial.deadline_extended);

    let new_deadline = original_deadline + 15 * DAY;
    s.client.extend_deadline(&id, &new_deadline);

    let updated = s.client.get_campaign(&id);
    assert_eq!(updated.deadline, new_deadline);
    assert!(updated.deadline_extended);
}

#[test]
fn second_deadline_extension_is_rejected() {
    let s = Setup::new();
    let id = s.campaign();
    let original_deadline = s.client.get_campaign(&id).deadline;

    // First extension succeeds
    s.client
        .extend_deadline(&id, &(original_deadline + 10 * DAY));

    // Second extension fails
    let res = s
        .client
        .try_extend_deadline(&id, &(original_deadline + 20 * DAY));
    assert_eq!(res, Err(Ok(Error::DeadlineAlreadyExtended)));
}

#[test]
fn deadline_extension_rules_enforced() {
    let s = Setup::new();
    let id = s.campaign();
    let original_deadline = s.client.get_campaign(&id).deadline;

    // Extension > 30 days rejected
    let res_too_long = s
        .client
        .try_extend_deadline(&id, &(original_deadline + 31 * DAY));
    assert_eq!(res_too_long, Err(Ok(Error::InvalidDeadlineExtension)));

    // Extension <= current deadline rejected
    let res_past = s.client.try_extend_deadline(&id, &original_deadline);
    assert_eq!(res_past, Err(Ok(Error::InvalidDeadlineExtension)));

    let res_earlier = s
        .client
        .try_extend_deadline(&id, &(original_deadline - DAY));
    assert_eq!(res_earlier, Err(Ok(Error::InvalidDeadlineExtension)));
}

#[test]
fn deadline_extension_fails_after_deadline_expired() {
    let s = Setup::new();
    let id = s.campaign();
    let original_deadline = s.client.get_campaign(&id).deadline;

    s.pass_deadline();

    let res = s
        .client
        .try_extend_deadline(&id, &(original_deadline + 10 * DAY));
    assert_eq!(res, Err(Ok(Error::CampaignExpired)));
}

#[test]
fn deadline_extension_fails_on_cancelled_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let original_deadline = s.client.get_campaign(&id).deadline;

    s.client.cancel_campaign(&s.creator, &id);

    let res = s
        .client
        .try_extend_deadline(&id, &(original_deadline + 10 * DAY));
    assert_eq!(res, Err(Ok(Error::CampaignNotActive)));
}

#[test]
fn deadline_extension_requires_creator_auth() {
    let s = Setup::new();
    let id = s.campaign();
    let original_deadline = s.client.get_campaign(&id).deadline;

    let res = s
        .client
        .mock_auths(&[])
        .try_extend_deadline(&id, &(original_deadline + 10 * DAY));
    assert!(res.is_err());
}

#[test]
fn verifier_can_reject_milestone_with_reason() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &500);

    let reason = String::from_str(&s.env, "ipfs://rejection-reason");
    let res = s.client.try_reject_milestone(&id, &reason);
    assert_eq!(res, Ok(Ok(())));

    // Verify event was emitted
    assert!(
        !s.env.events().all().events().is_empty(),
        "MilestoneRejected event not emitted"
    );

    // Verify no state change to funds or milestones progress
    let c = s.client.get_campaign(&id);
    assert_eq!(c.raised, 500);
    assert_eq!(c.released, 0);
    assert_eq!(c.milestones_released, 0);
    assert_eq!(c.status, CampaignStatus::Active);
    assert_eq!(s.token.balance(&s.beneficiary), 0);
}

#[test]
fn reject_milestone_requires_verifier_auth() {
    let s = Setup::new();
    let id = s.campaign();
    let reason = String::from_str(&s.env, "ipfs://rejection-reason");

    let res = s.client.mock_auths(&[]).try_reject_milestone(&id, &reason);
    assert!(res.is_err());
}

#[test]
fn reject_milestone_rejects_unregistered_verifier() {
    let s = Setup::new();
    let id = s.campaign();
    s.client.remove_verifier(&s.verifier);

    let reason = String::from_str(&s.env, "ipfs://rejection-reason");
    let res = s.client.try_reject_milestone(&id, &reason);
    assert_eq!(res, Err(Ok(Error::NotVerifier)));
}

#[test]
fn reject_milestone_rejects_long_reason_uri() {
    let s = Setup::new();
    let id = s.campaign();

    // 257-byte string exceeds MAX_URI_LENGTH (256)
    let long_uri = String::from_str(
        &s.env,
        "https://example.com/very/long/rejection/path/that/exceeds/the/maximum/allowed/length/of/two/hundred/and/fifty/six/bytes/---------------------------------------------------------------------------------------------------------------------------------------------",
    );
    let res = s.client.try_reject_milestone(&id, &long_uri);
    assert_eq!(res, Err(Ok(Error::UriTooLong)));
}

#[test]
fn reject_milestone_fails_for_expired_or_cancelled_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let reason = String::from_str(&s.env, "ipfs://rejection-reason");

    // Cancelled campaign
    s.client.cancel_campaign(&s.creator, &id);
    assert_eq!(
        s.client.try_reject_milestone(&id, &reason),
        Err(Ok(Error::CampaignNotActive))
    );

    // Expired campaign
    let s2 = Setup::new();
    let id2 = s2.campaign();
    let reason2 = String::from_str(&s2.env, "ipfs://rejection-reason");
    s2.pass_deadline();
    assert_eq!(
        s2.client.try_reject_milestone(&id2, &reason2),
        Err(Ok(Error::CampaignExpired))
    );
}

#[test]
fn reject_milestone_fails_when_all_milestones_released() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // Release all 3 milestones
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());

    let c = s.client.get_campaign(&id);
    assert_eq!(c.status, CampaignStatus::Completed);

    let reason = String::from_str(&s.env, "ipfs://rejection-reason");
    assert_eq!(
        s.client.try_reject_milestone(&id, &reason),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn platform_fee_defaults_to_zero_and_treasury_to_none() {
    let s = Setup::new();
    assert_eq!(s.client.platform_fee(), 0);
    assert_eq!(s.client.treasury(), None);
}

#[test]
fn admin_can_set_platform_fee_and_treasury() {
    let s = Setup::new();
    let treasury = Address::generate(&s.env);

    s.client.set_platform_fee(&150);
    assert!(
        !s.env.events().all().events().is_empty(),
        "PlatformFeeUpdated event should be emitted"
    );
    assert_eq!(s.client.platform_fee(), 150);

    s.client.set_treasury(&treasury);
    assert!(
        !s.env.events().all().events().is_empty(),
        "TreasuryUpdated event should be emitted"
    );
    assert_eq!(s.client.treasury(), Some(treasury));
}

#[test]
fn platform_fee_capped_at_two_percent() {
    let s = Setup::new();

    // 200 bps (2%) succeeds
    let res_valid = s.client.try_set_platform_fee(&200);
    assert_eq!(res_valid, Ok(Ok(())));
    assert_eq!(s.client.platform_fee(), 200);

    // 201 bps (>2%) fails with InvalidAmount
    let res_invalid = s.client.try_set_platform_fee(&201);
    assert_eq!(res_invalid, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn non_admin_cannot_set_fee_or_treasury() {
    let s = Setup::new();
    let treasury = Address::generate(&s.env);

    let res_fee = s.client.mock_auths(&[]).try_set_platform_fee(&100);
    assert!(res_fee.is_err());

    let res_tres = s.client.mock_auths(&[]).try_set_treasury(&treasury);
    assert!(res_tres.is_err());
}

#[test]
fn milestone_release_deducts_fee_and_sends_to_treasury() {
    let s = Setup::new();
    let treasury = Address::generate(&s.env);
    s.client.set_platform_fee(&200); // 2% fee
    s.client.set_treasury(&treasury);

    let id = s.campaign(); // milestones: 300, 300, 400
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // Approve milestone 0 (amount = 300)
    // Fee = 300 * 200 / 10000 = 6
    // Beneficiary receives 300 - 6 = 294
    // Treasury receives 6
    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 300);

    assert_eq!(s.token.balance(&s.beneficiary), 294);
    assert_eq!(s.token.balance(&treasury), 6);
    assert_eq!(s.token.balance(&s.client.address), 700);

    let c = s.client.get_campaign(&id);
    assert_eq!(c.released, 300);
}

#[test]
fn refund_math_unaffected_by_platform_fee() {
    let s = Setup::new();
    let treasury = Address::generate(&s.env);
    s.client.set_platform_fee(&200); // 2%
    s.client.set_treasury(&treasury);

    let id = s.campaign(); // 300, 300, 400
    let donor_a = s.donor(600);
    let donor_b = s.donor(400);

    s.client.donate(&donor_a, &id, &600);
    s.client.donate(&donor_b, &id, &400);

    // Release milestone 0: 300 total leaves escrow (294 to beneficiary, 6 to treasury)
    s.client.approve_milestone(&id, &s.proof());

    // Cancel campaign so remaining 700 is refundable
    s.client.cancel_campaign(&s.creator, &id);

    // Donor A refund: 600 * (1000 - 300) / 1000 = 420
    let refund_a = s.client.refund(&donor_a, &id);
    assert_eq!(refund_a, 420);
    assert_eq!(s.token.balance(&donor_a), 420);

    // Donor B refund: 400 * (1000 - 300) / 1000 = 280
    let refund_b = s.client.refund(&donor_b, &id);
    assert_eq!(refund_b, 280);
    assert_eq!(s.token.balance(&donor_b), 280);

    // Total refunded = 420 + 280 = 700. Remaining in contract = 0.
    assert_eq!(s.token.balance(&s.client.address), 0);
}

#[test]
fn remaining_to_goal_returns_correct_amount_and_zero_when_funded() {
    let s = Setup::new();
    let id = s.campaign(); // goal is 1000

    // Initial state: goal - raised = 1000 - 0 = 1000
    assert_eq!(s.client.remaining_to_goal(&id), 1000);

    // Donate 400: remaining is 600
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &400);
    assert_eq!(s.client.remaining_to_goal(&id), 600);

    // Donate remaining 600: goal reached, remaining is 0
    s.client.donate(&donor, &id, &600);
    assert_eq!(s.client.remaining_to_goal(&id), 0);
}

#[test]
fn remaining_to_goal_returns_campaign_not_found_for_unknown_id() {
    let s = Setup::new();
    let res = s.client.try_remaining_to_goal(&99_999);
    assert_eq!(res, Err(Ok(Error::CampaignNotFound)));
}

#[test]
fn next_milestone_returns_correct_milestone_across_approvals() {
    let s = Setup::new();
    let id = s.campaign(); // milestones: [300, 300, 400]
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // After 0 approvals: next milestone is index 0 with amount 300
    assert_eq!(s.client.next_milestone(&id), Some((0, 300)));

    // After 1 approval: next milestone is index 1 with amount 300
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.client.next_milestone(&id), Some((1, 300)));

    // After 2 approvals: next milestone is index 2 with amount 400
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.client.next_milestone(&id), Some((2, 400)));

    // After all 3 approvals: campaign is completed, returns None
    s.client.approve_milestone(&id, &s.proof());
    let c = s.client.get_campaign(&id);
    assert_eq!(c.status, CampaignStatus::Completed);
    assert_eq!(s.client.next_milestone(&id), None);
}

#[test]
fn next_milestone_returns_campaign_not_found_for_unknown_id() {
    let s = Setup::new();
    let res = s.client.try_next_milestone(&99_999);
    assert_eq!(res, Err(Ok(Error::CampaignNotFound)));
}

// ─── Optional minimum donation ────────────────────────────────────────────────

#[test]
fn donations_below_minimum_fail() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(100);
    let donor = s.donor(1000);

    let res = s.client.try_donate(&donor, &id, &50);
    assert_eq!(res, Err(Ok(Error::DonationBelowMinimum)));
}

#[test]
fn donations_at_or_above_minimum_succeed() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(100);
    let donor = s.donor(1000);

    let accepted_exact = s.client.donate(&donor, &id, &100);
    assert_eq!(accepted_exact, 100);

    let accepted_above = s.client.donate(&donor, &id, &150);
    assert_eq!(accepted_above, 150);
    assert_eq!(s.client.get_campaign(&id).raised, 250);
}

#[test]
fn final_smaller_donation_reaching_goal_accepted() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(100);
    let donor1 = s.donor(1000);
    let donor2 = s.donor(100);

    // Goal is 1000. Donate 950 first (above minimum of 100).
    s.client.donate(&donor1, &id, &950);
    assert_eq!(s.client.get_campaign(&id).raised, 950);

    // Remaining needed is 50. Minimum donation is 100.
    // A donation of 50 reaches the goal exactly and must be accepted.
    let accepted = s.client.donate(&donor2, &id, &50);
    assert_eq!(accepted, 50);
    assert_eq!(s.client.get_campaign(&id).raised, 1000);
}

#[test]
fn final_smaller_donation_exceeding_remaining_goal_accepted() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(100);
    let donor1 = s.donor(1000);
    let donor2 = s.donor(100);

    // Goal is 1000. Donate 960 first.
    s.client.donate(&donor1, &id, &960);
    assert_eq!(s.client.get_campaign(&id).raised, 960);

    // Remaining is 40. Donor offers 60 (below minimum 100, but exceeds remaining).
    let accepted = s.client.donate(&donor2, &id, &60);
    assert_eq!(accepted, 40);
    assert_eq!(s.client.get_campaign(&id).raised, 1000);
}

#[test]
fn final_donation_below_minimum_that_does_not_reach_goal_fails() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(100);
    let donor1 = s.donor(1000);
    let donor2 = s.donor(100);

    // Goal is 1000. Donate 950 first. Remaining is 50.
    s.client.donate(&donor1, &id, &950);

    // Donor offers 30 (below minimum 100 and does NOT reach remaining 50).
    let res = s.client.try_donate(&donor2, &id, &30);
    assert_eq!(res, Err(Ok(Error::DonationBelowMinimum)));
}

#[test]
fn minimum_donation_of_zero_behaves_as_normal() {
    let s = Setup::new();
    let id = s.campaign_with_min_donation(0);
    let donor = s.donor(100);

    // Any donation at or above MIN_DONATION_AMOUNT is accepted when min_donation is 0.
    let accepted = s.client.donate(&donor, &id, &MIN_DONATION_AMOUNT);
    assert_eq!(accepted, MIN_DONATION_AMOUNT);
}

#[test]
fn test_donate_dust_spam_rejected() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(100);

    // 1-stroop dust donation must be rejected with InvalidAmount
    let res = s.client.try_donate(&donor, &id, &1);
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_donate_exact_minimum_succeeds() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(100);

    // Donating exactly MIN_DONATION_AMOUNT must succeed
    let accepted = s.client.donate(&donor, &id, &MIN_DONATION_AMOUNT);
    assert_eq!(accepted, MIN_DONATION_AMOUNT);
    assert_eq!(s.client.get_campaign(&id).raised, MIN_DONATION_AMOUNT);
    assert_eq!(s.client.contribution_of(&id, &donor), MIN_DONATION_AMOUNT);
}

#[test]
fn negative_min_donation_is_rejected() {
    let s = Setup::new();
    let res = s.client.try_create_campaign_min_donation(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://flood-relief"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 300, 300, 400],
        &-1,
    );
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn min_donation_stored_in_campaign_record() {
    let s = Setup::new();

    // Default create_campaign has min_donation = 0.
    let id_default = s.campaign();
    assert_eq!(s.client.get_campaign(&id_default).min_donation, 0);

    // Configured min_donation is preserved.
    let id_custom = s.campaign_with_min_donation(250);
    assert_eq!(s.client.get_campaign(&id_custom).min_donation, 250);
}

// ─── Admin transfer tests: old admin access revoked ─────────────────────────

#[test]
fn old_admin_cannot_add_verifier_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let candidate = Address::generate(&s.env);
    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "add_verifier",
            args: (candidate.clone(),).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let expected_auth_err = soroban_sdk::Error::from_type_and_code(
        soroban_sdk::xdr::ScErrorType::Context,
        soroban_sdk::xdr::ScErrorCode::InvalidAction,
    );
    let res = s.client.mock_auths(&[auth]).try_add_verifier(&candidate);
    let err = res.unwrap_err().unwrap();
    assert_eq!(err, expected_auth_err);
    assert!(!s.client.is_verifier(&candidate));

    // New admin can add verifier
    s.client.add_verifier(&candidate);
    assert!(s.client.is_verifier(&candidate));
}

#[test]
fn old_admin_cannot_remove_verifier_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let verifier = Address::generate(&s.env);
    s.client.add_verifier(&verifier);
    assert!(s.client.is_verifier(&verifier));

    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "remove_verifier",
            args: (verifier.clone(),).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let expected_auth_err = soroban_sdk::Error::from_type_and_code(
        soroban_sdk::xdr::ScErrorType::Context,
        soroban_sdk::xdr::ScErrorCode::InvalidAction,
    );
    let res = s.client.mock_auths(&[auth]).try_remove_verifier(&verifier);
    let err = res.unwrap_err().unwrap();
    assert_eq!(err, expected_auth_err);
    assert!(s.client.is_verifier(&verifier));

    // New admin can remove verifier
    s.client.remove_verifier(&verifier);
    assert!(!s.client.is_verifier(&verifier));
}

#[test]
fn old_admin_cannot_set_bond_requirement_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "set_bond_requirement",
            args: (500i128,).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let res = s.client.mock_auths(&[auth]).try_set_bond_requirement(&500);
    assert_eq!(res.unwrap_err(), Err(InvokeError::Abort));
    assert_eq!(s.client.bond_requirement(), 0);

    // New admin can set bond requirement
    s.client.set_bond_requirement(&500);
    assert_eq!(s.client.bond_requirement(), 500);
}

#[test]
fn old_admin_cannot_set_bond_withdraw_delay_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "set_bond_withdraw_delay",
            args: (86400u64,).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let expected_auth_err = soroban_sdk::Error::from_type_and_code(
        soroban_sdk::xdr::ScErrorType::Context,
        soroban_sdk::xdr::ScErrorCode::InvalidAction,
    );
    let res = s
        .client
        .mock_auths(&[auth])
        .try_set_bond_withdraw_delay(&86400);
    let err = res.unwrap_err().unwrap();
    assert_eq!(err, expected_auth_err);
    assert_eq!(s.client.bond_withdraw_delay(), 0);

    // New admin can set bond withdraw delay
    s.client.set_bond_withdraw_delay(&86400);
    assert_eq!(s.client.bond_withdraw_delay(), 86400);
}

#[test]
fn old_admin_cannot_slash_verifier_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();

    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "slash_verifier",
            args: (v.clone(), id, 150i128).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let res = s
        .client
        .mock_auths(&[auth])
        .try_slash_verifier(&v, &id, &150);
    assert_eq!(res.unwrap_err(), Err(InvokeError::Abort));
    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 400);

    // New admin can slash verifier
    s.client.slash_verifier(&v, &id, &150);
    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 250);
    assert_eq!(s.client.slashed_recovery(&id), 150);
}

#[test]
fn old_admin_cannot_reassign_verifier_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let id = s.campaign();
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&new_v);

    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "reassign_verifier",
            args: (id, new_v.clone()).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let res = s
        .client
        .mock_auths(&[auth])
        .try_reassign_verifier(&id, &new_v);
    assert_eq!(res.unwrap_err(), Err(InvokeError::Abort));
    assert_eq!(s.client.get_campaign(&id).verifier, s.verifier);

    // New admin can reassign verifier
    s.client.reassign_verifier(&id, &new_v);
    assert_eq!(s.client.get_campaign(&id).verifier, new_v);
}

#[test]
fn cancelled_campaign_rejects_new_activity() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    let sponsor = s.donor(10_000);

    s.client.cancel_campaign(&s.creator, &id);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);

    assert_eq!(
        s.client.try_donate(&donor, &id, &100),
        Err(Ok(Error::CampaignNotActive))
    );
    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignNotActive))
    );

    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&donor, &s.client.address, &100, &expiry);
    assert_eq!(
        s.client.try_create_pledge(&donor, &id, &100),
        Err(Ok(Error::CampaignNotActive))
    );

    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn completed_campaign_rejects_new_activity() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    let sponsor = s.donor(10_000);

    // Fully fund and release all milestones
    s.client.donate(&donor, &id, &1_000);
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Completed);

    assert_eq!(
        s.client.try_donate(&donor, &id, &100),
        Err(Ok(Error::CampaignNotActive))
    );

    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&donor, &s.client.address, &100, &expiry);
    assert_eq!(
        s.client.try_create_pledge(&donor, &id, &100),
        Err(Ok(Error::CampaignNotActive))
    );

    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500),
        Err(Ok(Error::CampaignNotActive))
    );
    // Note: try_approve_milestone against a Completed campaign is already tested in milestones_release_in_order_and_complete.
}

#[test]
fn approval_before_cooldown_gap_fails_with_error() {
    let s = Setup::new();
    let cooldown_secs = 3_600; // 1 hour
    let id = s.campaign_with_cooldown(cooldown_secs);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);

    // Initial last approval time should be 0
    assert_eq!(s.client.last_approval_time(&id), 0);
    assert_eq!(s.client.approval_cooldown(&id), cooldown_secs);

    // First milestone approval succeeds immediately
    let initial_time = s.env.ledger().timestamp();
    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 300);

    // Last approval time is stored and visible
    assert_eq!(s.client.last_approval_time(&id), initial_time);
    assert_eq!(s.client.get_campaign(&id).last_approval_time, initial_time);

    // Immediate second milestone approval fails with ApprovalCooldownNotMet
    let res = s.client.try_approve_milestone(&id, &s.proof());
    assert_eq!(res, Err(Ok(Error::ApprovalCooldownNotMet)));

    // Approval before cooldown gap has passed (after 1800s) still fails
    s.env.ledger().set_timestamp(initial_time + 1_800);
    let res_halfway = s.client.try_approve_milestone(&id, &s.proof());
    assert_eq!(res_halfway, Err(Ok(Error::ApprovalCooldownNotMet)));

    // Timestamp jump to exactly the cooldown time allows approval
    s.env.ledger().set_timestamp(initial_time + cooldown_secs);
    let released_second = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released_second, 300);

    // Last approval time updated
    let second_time = initial_time + cooldown_secs;
    assert_eq!(s.client.last_approval_time(&id), second_time);
    assert_eq!(s.client.get_campaign(&id).last_approval_time, second_time);
}

#[test]
fn approval_cooldown_of_zero_behaves_as_normal() {
    let s = Setup::new();
    let id = s.campaign_with_cooldown(0);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);

    // Milestone 0 and 1 approved in the very same timestamp without error
    let released_first = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released_first, 300);

    let released_second = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released_second, 300);
}

#[test]
fn approval_cooldown_and_last_approval_time_return_campaign_not_found() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_approval_cooldown(&999),
        Err(Ok(Error::CampaignNotFound))
    );
    assert_eq!(
        s.client.try_last_approval_time(&999),
        Err(Ok(Error::CampaignNotFound))
    );
}

#[test]
fn cancel_campaign_with_reason_by_creator_or_admin_succeeds() {
    let s = Setup::new();
    let id = s.campaign();
    let reason = String::from_str(&s.env, "ipfs://reason-severe-weather");

    // Stranger cannot cancel with reason
    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.client
            .try_cancel_campaign_with_reason(&stranger, &id, &reason),
        Err(Ok(Error::Unauthorized))
    );

    // Creator can cancel with reason
    s.client
        .cancel_campaign_with_reason(&s.creator, &id, &reason);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);

    // Second cancel fails because campaign is already cancelled
    assert_eq!(
        s.client
            .try_cancel_campaign_with_reason(&s.creator, &id, &reason),
        Err(Ok(Error::CampaignNotActive))
    );

    // Admin can cancel another campaign with reason
    let id2 = s.campaign();
    let admin_reason = String::from_str(&s.env, "ipfs://safety-intervention");
    s.client
        .cancel_campaign_with_reason(&s.admin, &id2, &admin_reason);
    assert_eq!(
        s.client.get_campaign(&id2).status,
        CampaignStatus::Cancelled
    );

    // Existing cancel_campaign without reason still works
    let id3 = s.campaign();
    s.client.cancel_campaign(&s.creator, &id3);
    assert_eq!(
        s.client.get_campaign(&id3).status,
        CampaignStatus::Cancelled
    );
}

#[test]
fn cancel_campaign_with_reason_validates_uri_length_and_existence() {
    let s = Setup::new();
    let id = s.campaign();

    // 257-byte reason URI exceeds MAX_URI_LENGTH (256)
    let long_reason = String::from_str(
        &s.env,
        "https://example.com/long-cancel-reason-uri-exceeding-two-hundred-and-fifty-six-bytes-limit-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding",
    );
    assert_eq!(
        s.client
            .try_cancel_campaign_with_reason(&s.creator, &id, &long_reason),
        Err(Ok(Error::UriTooLong))
    );

    // Non-existent campaign
    let valid_reason = String::from_str(&s.env, "ipfs://reason");
    assert_eq!(
        s.client
            .try_cancel_campaign_with_reason(&s.creator, &999, &valid_reason),
        Err(Ok(Error::CampaignNotFound))
    );
}

#[test]
fn verifier_profile_uri_recorded_and_queried() {
    let s = Setup::new();
    let profile = String::from_str(&s.env, "https://redcross.org/verifier-profile");

    // Initially, profile is None
    assert_eq!(s.client.verifier_profile(&s.verifier), None);

    // Admin sets profile URI for registered verifier
    s.client.set_verifier_profile(&s.verifier, &profile);
    assert_eq!(
        s.client.verifier_profile(&s.verifier),
        Some(profile.clone())
    );

    // Admin can update profile URI
    let updated_profile = String::from_str(&s.env, "https://redcross.org/updated-profile");
    s.client.set_verifier_profile(&s.verifier, &updated_profile);
    assert_eq!(
        s.client.verifier_profile(&s.verifier),
        Some(updated_profile)
    );

    // Unset profile returns None for an unregistered address
    let stranger = Address::generate(&s.env);
    assert_eq!(s.client.verifier_profile(&stranger), None);
}

#[test]
fn verifier_profile_only_admin_and_only_registered_verifiers() {
    let s = Setup::new();
    let profile = String::from_str(&s.env, "https://example.org/profile");
    let stranger = Address::generate(&s.env);

    // Fails for unregistered verifier address
    assert_eq!(
        s.client.try_set_verifier_profile(&stranger, &profile),
        Err(Ok(Error::NotVerifier))
    );

    // Non-admin cannot set profile
    let auth = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "set_verifier_profile",
            args: (s.verifier.clone(), profile.clone()).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res = s
        .client
        .mock_auths(&[auth])
        .try_set_verifier_profile(&s.verifier, &profile);
    assert_eq!(res.unwrap_err(), Err(InvokeError::Abort));

    // Reject URI exceeding MAX_URI_LENGTH (256)
    let long_uri = String::from_str(
        &s.env,
        "https://example.com/very-long-verifier-profile-uri-exceeding-two-hundred-and-fifty-six-bytes-limit-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding",
    );
    assert_eq!(
        s.client.try_set_verifier_profile(&s.verifier, &long_uri),
        Err(Ok(Error::UriTooLong))
    );
}

#[test]
fn donors_pagination_returns_stable_order_and_no_duplicates() {
    let s = Setup::new();
    let id = s.campaign();

    let d1 = s.donor(100);
    let d2 = s.donor(100);
    let d3 = s.donor(100);
    let d4 = s.donor(100);
    let d5 = s.donor(100);

    // Donate in order: d1, d2, d3, d4, d5
    s.client.donate(&d1, &id, &100);
    s.client.donate(&d2, &id, &100);
    s.client.donate(&d3, &id, &100);
    s.client.donate(&d4, &id, &100);
    s.client.donate(&d5, &id, &100);

    // Page 1: start 0, limit 2
    let page1 = s.client.donors(&id, &0, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap(), d1);
    assert_eq!(page1.get(1).unwrap(), d2);

    // Page 2: start 2, limit 2
    let page2 = s.client.donors(&id, &2, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap(), d3);
    assert_eq!(page2.get(1).unwrap(), d4);

    // Page 3: start 4, limit 2
    let page3 = s.client.donors(&id, &4, &2);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap(), d5);

    // Out of bounds start
    let empty_page = s.client.donors(&id, &5, &2);
    assert_eq!(empty_page.len(), 0);

    // Limit of zero
    let zero_page = s.client.donors(&id, &0, &0);
    assert_eq!(zero_page.len(), 0);
}

#[test]
fn repeat_donors_are_not_added_twice() {
    let s = Setup::new();
    let id = s.campaign();

    let d1 = s.donor(300);
    let d2 = s.donor(300);
    let d3 = s.donor(300);

    // d1 donates first time
    s.client.donate(&d1, &id, &100);
    // d2 donates first time
    s.client.donate(&d2, &id, &100);
    // d1 repeat donation
    s.client.donate(&d1, &id, &100);
    // d2 repeat donation
    s.client.donate(&d2, &id, &100);
    // d3 donates first time
    s.client.donate(&d3, &id, &100);

    let all = s.client.donors(&id, &0, &50);
    assert_eq!(all.len(), 3);
    assert_eq!(all.get(0).unwrap(), d1);
    assert_eq!(all.get(1).unwrap(), d2);
    assert_eq!(all.get(2).unwrap(), d3);
}

#[test]
fn donors_limit_above_cap_is_clamped() {
    let s = Setup::new();
    let id = s.campaign();

    let d1 = s.donor(100);
    let d2 = s.donor(100);
    s.client.donate(&d1, &id, &50);
    s.client.donate(&d2, &id, &50);

    // Request limit = 100 which exceeds MAX_DONORS_PAGE_LIMIT (50).
    // Should be clamped gracefully and return all 2 donors without failure.
    let res = s.client.donors(&id, &0, &100);
    assert_eq!(res.len(), 2);
    assert_eq!(res.get(0).unwrap(), d1);
    assert_eq!(res.get(1).unwrap(), d2);

    // Non-existent campaign returns empty vector
    let none_res = s.client.donors(&999, &0, &50);
    assert_eq!(none_res.len(), 0);
}

// ─── Property tests for sponsor matching math ─────────────────────────────────

#[derive(Clone, Copy)]
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x853c49e6748fea9b } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    fn gen_range(&mut self, min: i128, max: i128) -> i128 {
        if min >= max {
            return min;
        }
        let range = (max - min + 1) as u128;
        let val = (self.next_u64() as u128 | ((self.next_u64() as u128) << 64)) % range;
        min + val as i128
    }

    fn gen_u32_range(&mut self, min: u32, max: u32) -> u32 {
        if min >= max {
            return min;
        }
        let range = (max - min + 1) as u64;
        min + (self.next_u64() % range) as u32
    }
}

/// Simulation helper mirroring the contract's matching math in `apply_matching` and `return_sponsor_pool`.
struct MatchingSimState {
    goal: i128,
    raised: i128,
    cap: i128,
    deposited: i128,
    remaining: i128,
    matched: i128,
    ratio_bps: u32,
}

impl MatchingSimState {
    fn new(goal: i128, cap: i128, ratio_bps: u32) -> Self {
        Self {
            goal,
            raised: 0,
            cap,
            deposited: cap,
            remaining: cap,
            matched: 0,
            ratio_bps,
        }
    }

    fn donate(&mut self, amount: i128) -> i128 {
        if amount <= 0 || self.raised >= self.goal {
            return 0;
        }
        let accepted = amount.min(self.goal - self.raised);
        self.raised += accepted;
        accepted
    }

    fn apply_match(&mut self, donation_amount: i128) -> i128 {
        if donation_amount <= 0 {
            return 0;
        }
        let raw_match = match donation_amount.checked_mul(self.ratio_bps as i128) {
            Some(prod) => prod / 10_000,
            None => return 0,
        };
        if raw_match <= 0 {
            return 0;
        }
        let cap_remaining = self.cap - self.matched;
        let pool_limit = self.remaining;
        let campaign_headroom = self.goal - self.raised;

        let matched_amount = raw_match
            .min(cap_remaining)
            .min(pool_limit)
            .min(campaign_headroom);

        if matched_amount <= 0 {
            return 0;
        }

        self.remaining -= matched_amount;
        self.matched += matched_amount;
        self.raised += matched_amount;
        matched_amount
    }

    fn return_pool(&mut self) -> i128 {
        let return_amount = self.remaining;
        self.remaining = 0;
        return_amount
    }
}

#[test]
fn prop_matching_never_exceeds_pool_cap() {
    let mut rng = SimpleRng::new(0x104_001);
    const CASES: usize = 1200;

    for case in 0..CASES {
        let ratio = match case {
            0 => 1,
            1 => 10_000,
            2..=100 => 1,
            101..=200 => 10_000,
            _ => rng.gen_u32_range(1, 10_000),
        };
        let cap = rng.gen_range(1, 1_000_000);
        let goal = rng.gen_range(cap / 2 + 1, cap * 5 + 10);
        let mut sim = MatchingSimState::new(goal, cap, ratio);

        let steps = rng.gen_range(1, 15);
        for _ in 0..steps {
            let donation = match rng.gen_range(0, 3) {
                0 => rng.gen_range(1, 10),
                1 => rng.gen_range(goal / 2, goal + 100),
                _ => rng.gen_range(1, goal * 2),
            };
            sim.donate(donation);
            sim.apply_match(donation);
            assert!(
                sim.matched <= sim.cap,
                "Case {case}: matched {} exceeded cap {}",
                sim.matched,
                sim.cap
            );
        }
    }
}

#[test]
fn prop_matching_never_passes_goal() {
    let mut rng = SimpleRng::new(0x104_002);
    const CASES: usize = 1200;

    for case in 0..CASES {
        let ratio = match case {
            0 => 1,
            1 => 10_000,
            2..=100 => 1,
            101..=200 => 10_000,
            _ => rng.gen_u32_range(1, 10_000),
        };
        let goal = rng.gen_range(100, 1_000_000);
        let cap = rng.gen_range(goal / 2, goal * 2);
        let mut sim = MatchingSimState::new(goal, cap, ratio);

        // Intentionally test amounts near the goal (e.g. raised within 1..10 of goal)
        let near_goal_donation = goal - rng.gen_range(1, 10);
        sim.donate(near_goal_donation);
        assert!(sim.raised <= sim.goal);

        let steps = rng.gen_range(1, 10);
        for _ in 0..steps {
            let donation = rng.gen_range(1, goal);
            sim.donate(donation);
            sim.apply_match(donation);
            assert!(
                sim.raised <= sim.goal,
                "Case {case}: raised {} passed goal {}",
                sim.raised,
                sim.goal
            );
        }
    }
}

#[test]
fn prop_matching_deposited_equals_remaining_plus_matched() {
    let mut rng = SimpleRng::new(0x104_003);
    const CASES: usize = 1200;

    for case in 0..CASES {
        let ratio = match case {
            0 => 1,
            1 => 10_000,
            2..=100 => 1,
            101..=200 => 10_000,
            _ => rng.gen_u32_range(1, 10_000),
        };
        let cap = rng.gen_range(1, 2_000_000);
        let goal = rng.gen_range(10, cap * 3);
        let mut sim = MatchingSimState::new(goal, cap, ratio);

        let steps = rng.gen_range(1, 20);
        for _ in 0..steps {
            let donation = rng.gen_range(1, goal + 100);
            sim.donate(donation);
            sim.apply_match(donation);
            assert_eq!(
                sim.deposited,
                sim.remaining + sim.matched,
                "Case {case}: deposited {} != remaining {} + matched {}",
                sim.deposited,
                sim.remaining,
                sim.matched
            );
        }
    }
}

#[test]
fn prop_matching_returns_pay_back_exactly_remaining() {
    let mut rng = SimpleRng::new(0x104_004);
    const CASES: usize = 1200;

    for case in 0..CASES {
        let ratio = match case {
            0 => 1,
            1 => 10_000,
            2..=100 => 1,
            101..=200 => 10_000,
            _ => rng.gen_u32_range(1, 10_000),
        };
        let cap = rng.gen_range(1, 1_000_000);
        let goal = rng.gen_range(10, cap * 2);
        let mut sim = MatchingSimState::new(goal, cap, ratio);

        let steps = rng.gen_range(0, 10);
        for _ in 0..steps {
            let donation = rng.gen_range(1, goal);
            sim.donate(donation);
            sim.apply_match(donation);
        }

        let expected_return = sim.remaining;
        let returned = sim.return_pool();
        assert_eq!(
            returned, expected_return,
            "Case {case}: returned {} != expected remaining {}",
            returned, expected_return
        );
        assert_eq!(sim.remaining, 0);
    }
}

#[test]
fn test_sponsor_matching_properties_contract_integration() {
    // End-to-end integration test on the Soroban contract verifying the four invariants:
    // 1. matched never exceeds cap
    // 2. never passes the goal
    // 3. deposited == remaining + matched
    // 4. return pays back exactly remaining
    let s = Setup::new();
    let sponsor = s.donor(1_000_000);
    let donor = s.donor(1_000_000);

    // Test ratio 1, ratio 10_000, and amount near the goal
    for &(ratio, cap, goal, near_goal_gap) in &[
        (1u32, 500i128, 1_000i128, 5i128),
        (10_000u32, 500i128, 1_000i128, 50i128),
        (5_000u32, 200i128, 1_000i128, 2i128),
    ] {
        let id = s.client.create_campaign(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://prop-contract-test"),
            &(s.env.ledger().timestamp() + 30 * DAY),
            &vec![&s.env, goal],
        );

        s.client
            .deposit_sponsor_pool(&sponsor, &id, &ratio, &cap, &cap);
        let initial_pool = s.client.get_sponsor_pool(&id, &sponsor).unwrap();
        assert_eq!(initial_pool.deposited, cap);
        assert_eq!(initial_pool.remaining, cap);
        assert_eq!(initial_pool.matched, 0);

        // Bring campaign close to goal
        let initial_donation = goal - near_goal_gap;
        s.client.donate(&donor, &id, &initial_donation);

        // Apply matching
        s.client
            .apply_matching(&sponsor, &id, &donor, &initial_donation);

        let pool = s.client.get_sponsor_pool(&id, &sponsor).unwrap();
        let campaign = s.client.get_campaign(&id);

        // Invariant 1: matched never exceeds cap
        assert!(pool.matched <= pool.cap);
        // Invariant 2: never passes the goal
        assert!(campaign.raised <= campaign.goal);
        // Invariant 3: deposited == remaining + matched
        assert_eq!(pool.deposited, pool.remaining + pool.matched);

        // Invariant 4: return pays back exactly remaining
        s.pass_deadline();
        let sponsor_bal_before = s.token.balance(&sponsor);
        let expected_remaining = pool.remaining;
        let returned = s.client.return_sponsor_pool(&sponsor, &id);
        assert_eq!(returned, expected_remaining);
        assert_eq!(
            s.token.balance(&sponsor),
            sponsor_bal_before + expected_remaining
        );
    }
}

// ─── Accounting invariant tests across random call sequences ──────────────────

#[test]
fn test_accounting_invariants_across_random_call_sequences() {
    const SEQUENCES: usize = 200;
    const STEPS_PER_SEQ: usize = 30;

    for seq_idx in 0..SEQUENCES {
        let seed = 0x105_0000u64.wrapping_add(seq_idx as u64);
        let mut rng = SimpleRng::new(seed);

        let s = Setup::new();
        let sponsor = s.donor(100_000);
        let donor1 = s.donor(100_000);
        let donor2 = s.donor(100_000);
        let donors = [donor1, donor2];

        // Campaign 0: 3 milestones (total 1000)
        let c0 = s.campaign();
        // Campaign 1: 2 milestones (total 800)
        let c1 = s.client.create_campaign(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://c1"),
            &(s.env.ledger().timestamp() + 30 * DAY),
            &vec![&s.env, 400, 400],
        );
        let campaigns = [c0, c1];

        // Deposit sponsor pools on both campaigns
        let pool_cap_0 = 500i128;
        let pool_cap_1 = 400i128;
        s.client
            .deposit_sponsor_pool(&sponsor, &c0, &5_000, &pool_cap_0, &pool_cap_0);
        s.client
            .deposit_sponsor_pool(&sponsor, &c1, &10_000, &pool_cap_1, &pool_cap_1);

        // Track escrow per campaign
        let mut escrow = [0i128, 0i128];
        let mut unreturned_pools = [pool_cap_0, pool_cap_1];
        let mut donated = [[0i128; 2]; 2]; // [campaign_idx][donor_idx]
        let mut refunded = [[0i128; 2]; 2];

        for step in 0..STEPS_PER_SEQ {
            let action = rng.gen_u32_range(0, 5);
            let camp_idx = (rng.next_u64() % 2) as usize;
            let camp_id = campaigns[camp_idx];
            let donor_idx = (rng.next_u64() % 2) as usize;
            let donor = &donors[donor_idx];

            match action {
                0 => {
                    // Donate
                    let amount = rng.gen_range(1, 300);
                    if let Ok(Ok(accepted)) = s.client.try_donate(donor, &camp_id, &amount) {
                        escrow[camp_idx] += accepted;
                        donated[camp_idx][donor_idx] += accepted;
                    }
                }
                1 => {
                    // Apply matching
                    let amount = rng.gen_range(1, 200);
                    if let Ok(Ok(matched)) = s
                        .client
                        .try_apply_matching(&sponsor, &camp_id, donor, &amount)
                    {
                        escrow[camp_idx] += matched;
                        unreturned_pools[camp_idx] -= matched;
                    }
                }
                2 => {
                    // Approve milestone
                    if let Ok(Ok(released)) = s.client.try_approve_milestone(&camp_id, &s.proof()) {
                        escrow[camp_idx] -= released;
                    }
                }
                3 => {
                    // Cancel campaign
                    let _ = s.client.try_cancel_campaign(&s.creator, &camp_id);
                }
                4 => {
                    // Refund
                    if let Ok(Ok(refund_amount)) = s.client.try_refund(donor, &camp_id) {
                        escrow[camp_idx] -= refund_amount;
                        refunded[camp_idx][donor_idx] += refund_amount;
                    }
                }
                5 => {
                    // Return sponsor pool
                    if let Ok(Ok(returned)) = s.client.try_return_sponsor_pool(&sponsor, &camp_id) {
                        unreturned_pools[camp_idx] -= returned;
                    }
                }
                _ => {}
            }

            // Invariant 1: raised >= released for all campaigns
            for (idx, cid) in campaigns.iter().enumerate() {
                let camp = s.client.get_campaign(cid);
                assert!(
                    camp.raised >= camp.released,
                    "Seq {} (seed 0x{:x}) step {}: Campaign {} raised {} < released {}",
                    seq_idx,
                    seed,
                    step,
                    idx,
                    camp.raised,
                    camp.released
                );
            }

            // Invariant 2: contract token balance matches sum of escrow across campaigns plus unreturned pools and bonds
            let contract_balance = s.token.balance(&s.client.address);
            let expected_balance =
                escrow[0] + escrow[1] + unreturned_pools[0] + unreturned_pools[1];
            assert_eq!(
                contract_balance, expected_balance,
                "Seq {} (seed 0x{:x}) step {}: Contract balance {} != expected {}",
                seq_idx, seed, step, contract_balance, expected_balance
            );

            // Invariant 3: no donor can refund more than they gave
            for c_i in 0..2 {
                for d_i in 0..2 {
                    assert!(
                        refunded[c_i][d_i] <= donated[c_i][d_i],
                        "Seq {} (seed 0x{:x}) step {}: Donor {} on campaign {} refunded {} > donated {}",
                        seq_idx,
                        seed,
                        step,
                        d_i,
                        c_i,
                        refunded[c_i][d_i],
                        donated[c_i][d_i]
                    );
                }
            }
        }
    }
}

// ─── Issue #125: pledge settlement is scoped to the campaign ─────────────────

mod pledge_scope {
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
}

// ─── Issue #129: bond withdrawal waits for the verifier's campaigns ──────────

mod bond_lock {
    use super::*;

    fn campaign_for(s: &Setup, verifier: &Address) -> u64 {
        s.client.create_campaign(
            &s.creator,
            &s.beneficiary,
            verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://bond-lock"),
            &(s.env.ledger().timestamp() + 30 * DAY),
            &vec![&s.env, 300, 300, 400],
        )
    }

    #[test]
    fn bond_is_locked_while_a_campaign_is_open_and_free_after_cancel() {
        let s = Setup::new();
        let v = s.bonded_verifier(1_000, 400);
        let id = campaign_for(&s, &v);
        s.client.deregister_verifier(&v, &v);

        assert_eq!(
            s.client.try_withdraw_bond(&v),
            Err(Ok(Error::VerifierHasActiveCampaigns))
        );
        // Still slashable while the campaign is open.
        s.client.slash_verifier(&v, &id, &100);
        assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 300);

        s.client.cancel_campaign(&s.creator, &id);
        assert_eq!(s.client.withdraw_bond(&v), 300);
    }

    #[test]
    fn bond_is_free_once_every_campaign_completes() {
        let s = Setup::new();
        let v = s.bonded_verifier(1_000, 400);
        let first = campaign_for(&s, &v);
        let second = campaign_for(&s, &v);
        let donor = s.donor(2_000);

        for id in [first, second] {
            s.client.donate(&donor, &id, &1_000);
            for _ in 0..3 {
                s.client.approve_milestone(&id, &s.proof());
            }
            // The count only reaches zero after the last campaign.
            if id == first {
                s.client.deregister_verifier(&v, &v);
                assert_eq!(
                    s.client.try_withdraw_bond(&v),
                    Err(Ok(Error::VerifierHasActiveCampaigns))
                );
                s.client.add_verifier(&v);
            }
        }
        s.client.deregister_verifier(&v, &v);
        assert_eq!(s.client.withdraw_bond(&v), 400);
    }

    #[test]
    fn reassigning_a_campaign_moves_the_lock_to_the_new_verifier() {
        let s = Setup::new();
        let old = s.bonded_verifier(1_000, 400);
        let new = s.bonded_verifier(1_000, 400);
        let id = campaign_for(&s, &old);
        s.client.deregister_verifier(&old, &old);
        s.client.deregister_verifier(&new, &new);
        s.client.add_verifier(&new);

        s.client.reassign_verifier(&id, &new);

        assert_eq!(s.client.withdraw_bond(&old), 400);
        assert_eq!(
            s.client.try_withdraw_bond(&new),
            Err(Ok(Error::VerifierHasActiveCampaigns))
        );
    }

    #[test]
    fn verifiers_without_campaigns_withdraw_as_before() {
        let s = Setup::new();
        let v = s.bonded_verifier(1_000, 400);
        s.client.deregister_verifier(&v, &v);
        assert_eq!(s.client.withdraw_bond(&v), 400);
    }
}

// ─── Issue #114: donor vote to cancel ────────────────────────────────────────

mod cancel_vote {
    use super::*;
    use crate::STALL_PERIOD;

    /// A campaign of 1000 funded 600 by `a` and 400 by `b`.
    fn funded(s: &Setup) -> (u64, Address, Address) {
        let id = s.campaign();
        let (a, b) = (s.donor(600), s.donor(400));
        s.client.donate(&a, &id, &600);
        s.client.donate(&b, &id, &400);
        (id, a, b)
    }

    fn jump(s: &Setup, seconds: u64) {
        let now = s.env.ledger().timestamp();
        s.env.ledger().set_timestamp(now + seconds);
    }

    #[test]
    fn votes_before_the_stall_period_are_recorded_and_cancel_later() {
        let s = Setup::new();
        let (id, a, _b) = funded(&s);

        // 600 of 1000 is over the threshold, but the campaign is not stalled yet.
        assert!(!s.client.vote_cancel(&a, &id));
        assert_eq!(s.client.cancel_vote_weight(&id), 600);
        assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Active);
        assert_eq!(
            s.client.try_execute_vote_cancel(&id),
            Err(Ok(Error::NotStalled))
        );

        jump(&s, STALL_PERIOD);
        s.client.execute_vote_cancel(&id);
        assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);
        assert_eq!(s.client.refund(&a, &id), 600);
    }

    #[test]
    fn a_vote_cast_after_the_stall_period_cancels_immediately() {
        let s = Setup::new();
        let (id, a, _b) = funded(&s);
        jump(&s, STALL_PERIOD);

        assert!(s.client.vote_cancel(&a, &id));
        assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);
    }

    #[test]
    fn votes_are_weighted_by_contribution_and_counted_once() {
        let s = Setup::new();
        let (id, a, b) = funded(&s);
        jump(&s, STALL_PERIOD);

        // 400 of 1000 is below the 50% threshold.
        assert!(!s.client.vote_cancel(&b, &id));
        assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Active);
        assert_eq!(
            s.client.try_vote_cancel(&b, &id),
            Err(Ok(Error::AlreadyVoted))
        );
        assert_eq!(s.client.cancel_vote_weight(&id), 400);

        let outsider = s.donor(10);
        assert_eq!(
            s.client.try_vote_cancel(&outsider, &id),
            Err(Ok(Error::NothingToRefund))
        );

        // The second donor pushes it over.
        assert!(s.client.vote_cancel(&a, &id));
    }

    #[test]
    fn a_vote_can_be_withdrawn_and_cast_again() {
        let s = Setup::new();
        let (id, a, _b) = funded(&s);

        s.client.vote_cancel(&a, &id);
        s.client.withdraw_cancel_vote(&a, &id);
        assert_eq!(s.client.cancel_vote_weight(&id), 0);
        assert_eq!(
            s.client.try_withdraw_cancel_vote(&a, &id),
            Err(Ok(Error::NoVote))
        );

        jump(&s, STALL_PERIOD);
        assert_eq!(
            s.client.try_execute_vote_cancel(&id),
            Err(Ok(Error::VoteThresholdNotReached))
        );
        assert!(s.client.vote_cancel(&a, &id));
    }

    #[test]
    fn an_approved_milestone_restarts_the_stall_period() {
        let s = Setup::new();
        let (id, a, _b) = funded(&s);
        jump(&s, STALL_PERIOD - 1);
        s.client.approve_milestone(&id, &s.proof());

        jump(&s, 2);
        // Past the original deadline, but the approval moved the clock.
        assert!(!s.client.vote_cancel(&a, &id));
        assert_eq!(
            s.client.try_execute_vote_cancel(&id),
            Err(Ok(Error::NotStalled))
        );

        jump(&s, STALL_PERIOD);
        s.client.execute_vote_cancel(&id);
        assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);
    }

    #[test]
    fn cancelling_by_vote_releases_the_verifiers_bond_lock() {
        let s = Setup::new();
        let v = s.bonded_verifier(1_000, 400);
        let id = s.client.create_campaign(
            &s.creator,
            &s.beneficiary,
            &v,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://vote"),
            &(s.env.ledger().timestamp() + 30 * DAY),
            &vec![&s.env, 300, 300, 400],
        );
        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &1_000);
        s.client.deregister_verifier(&v, &v);
        assert_eq!(
            s.client.try_withdraw_bond(&v),
            Err(Ok(Error::VerifierHasActiveCampaigns))
        );

        jump(&s, STALL_PERIOD);
        assert!(s.client.vote_cancel(&donor, &id));
        assert_eq!(s.client.withdraw_bond(&v), 400);
    }
}

// ─── Issue #117: streaming release ───────────────────────────────────────────

mod streaming {
    use super::*;

    /// Milestones 300, 300, 400 where only the first streams, over 10 days.
    fn streamed(s: &Setup) -> u64 {
        s.client.create_campaign_with_streams(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://stream"),
            &(s.env.ledger().timestamp() + 30 * DAY),
            &vec![&s.env, 300, 300, 400],
            &vec![&s.env, 10, 0, 0],
        )
    }

    fn jump(s: &Setup, seconds: u64) {
        let now = s.env.ledger().timestamp();
        s.env.ledger().set_timestamp(now + seconds);
    }

    #[test]
    fn vested_amount_follows_time_and_can_be_claimed_repeatedly() {
        let s = Setup::new();
        let id = streamed(&s);
        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &1_000);

        s.client.approve_milestone(&id, &s.proof());
        // Nothing moves at approval.
        assert_eq!(s.token.balance(&s.beneficiary), 0);
        assert_eq!(
            s.client.try_claim_streamed(&id),
            Err(Ok(Error::NothingToClaim))
        );

        jump(&s, 5 * DAY);
        assert_eq!(s.client.claim_streamed(&id), 150);
        assert_eq!(s.token.balance(&s.beneficiary), 150);

        // A quarter of the remaining half: 2.5 days is 75.
        jump(&s, DAY * 5 / 2);
        assert_eq!(s.client.claim_streamed(&id), 75);

        // Long after the end the total is capped at the milestone.
        jump(&s, 100 * DAY);
        assert_eq!(s.client.claim_streamed(&id), 75);
        assert_eq!(s.token.balance(&s.beneficiary), 300);
        assert_eq!(
            s.client.try_claim_streamed(&id),
            Err(Ok(Error::NothingToClaim))
        );
    }

    #[test]
    fn lump_sum_milestones_still_pay_immediately() {
        let s = Setup::new();
        let id = streamed(&s);
        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &1_000);
        s.client.approve_milestone(&id, &s.proof());

        // Milestone 1 has no stream days.
        s.client.approve_milestone(&id, &s.proof());
        assert_eq!(s.token.balance(&s.beneficiary), 300);
    }

    #[test]
    fn cancel_keeps_the_vested_part_and_refunds_the_unvested_part() {
        let s = Setup::new();
        let id = streamed(&s);
        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &1_000);
        s.client.approve_milestone(&id, &s.proof());

        // 3 of 10 days: 90 of 300 has vested.
        jump(&s, 3 * DAY);
        s.client.cancel_campaign(&s.creator, &id);

        // Released shrinks from 300 to the vested 90, so 910 is refundable.
        assert_eq!(s.client.get_campaign(&id).released, 90);
        assert_eq!(s.client.refund(&donor, &id), 910);

        // The beneficiary can still claim what had vested, but nothing more,
        // however long they wait.
        jump(&s, 30 * DAY);
        assert_eq!(s.client.claim_streamed(&id), 90);
        assert_eq!(s.token.balance(&s.beneficiary), 90);
        assert_eq!(s.token.balance(&s.client.address), 0);
    }

    #[test]
    fn an_expired_campaign_settles_its_streams_on_the_first_refund() {
        let s = Setup::new();
        let id = streamed(&s);
        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &1_000);
        s.client.approve_milestone(&id, &s.proof());

        jump(&s, 2 * DAY);
        s.pass_deadline();
        // Vested at the deadline check: 31 days is more than the stream, so all
        // 300 has vested and the refund pool is the other 700.
        assert_eq!(s.client.refund(&donor, &id), 700);
        assert_eq!(s.client.claim_streamed(&id), 300);
    }

    #[test]
    fn stream_days_must_match_the_milestones() {
        let s = Setup::new();
        assert_eq!(
            s.client.try_create_campaign_with_streams(
                &s.creator,
                &s.beneficiary,
                &s.verifier,
                &s.token.address,
                &CampaignKind::Emergency,
                &String::from_str(&s.env, "ipfs://bad"),
                &(s.env.ledger().timestamp() + 30 * DAY),
                &vec![&s.env, 300, 300, 400],
                &vec![&s.env, 10],
            ),
            Err(Ok(Error::InvalidStream))
        );
    }
}

// ─── Overflow guards from #165 ───────────────────────────────────────────────

#[test]
fn refund_pro_rata_overflow_returns_invalid_amount() {
    let s = Setup::new();
    let donor1_amount: i128 = 2_000_000_000_000_000_000_000_000_000_000;
    let milestones = vec![&s.env, donor1_amount, donor1_amount];
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://overflow-refund"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &milestones,
    );

    let donor1 = s.donor(donor1_amount);
    let donor2 = s.donor(MIN_DONATION_AMOUNT);

    s.client.donate(&donor1, &id, &donor1_amount);
    s.client.donate(&donor2, &id, &MIN_DONATION_AMOUNT);

    s.client.cancel_campaign(&s.creator, &id);

    // donor1: contributed = donor1_amount, unreleased = donor1_amount + MIN_DONATION_AMOUNT.
    // contributed * unreleased overflows i128::MAX -> returns Err(Ok(Error::InvalidAmount))
    assert_eq!(
        s.client.try_refund(&donor1, &id),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        s.client.try_refund_for(&donor1, &id),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(s.client.refundable_amount(&id, &donor1), 0);

    // donor2: contributed = MIN_DONATION_AMOUNT, unreleased = donor1_amount + MIN_DONATION_AMOUNT.
    // MIN_DONATION_AMOUNT * (donor1_amount + MIN_DONATION_AMOUNT) does not overflow -> succeeds
    let refunded2 = s.client.refund(&donor2, &id);
    assert_eq!(refunded2, MIN_DONATION_AMOUNT);
}

#[test]
fn emergency_fast_track_cap_overflow_returns_invalid_amount() {
    let s = Setup::new();
    let large_goal = i128::MAX / 1000;
    let milestones = vec![&s.env, large_goal];
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://overflow-goal"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &milestones,
    );

    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &500);

    // Calling emergency_fast_track with an extreme goal overflows cap calculation and returns InvalidAmount
    assert_eq!(
        s.client.try_emergency_fast_track(&id),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn pledge_pull_overflow_skips_pledge_safely() {
    let s = Setup::new();
    let milestone_val = i128::MAX / 10;
    let milestones = vec![&s.env, milestone_val];
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://overflow-pledge"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &milestones,
    );

    let donor_overflow = s.donor(i128::MAX / 2);
    let donor_normal = s.donor(1_000);
    let donor_direct = s.donor(milestone_val);
    let expiry = s.env.ledger().sequence() + 1_000;

    let pledge_large = i128::MAX / 2;
    s.token
        .approve(&donor_overflow, &s.client.address, &pledge_large, &expiry);
    s.token
        .approve(&donor_normal, &s.client.address, &5, &expiry);

    let pid_overflow = s.client.create_pledge(&donor_overflow, &id, &pledge_large);
    let pid_normal = s.client.create_pledge(&donor_normal, &id, &5);

    // Direct donation covers milestone so that approval can succeed even if pledges are skipped
    s.client.donate(&donor_direct, &id, &milestone_val);

    // Initial balances
    assert_eq!(s.token.balance(&donor_overflow), pledge_large);
    assert_eq!(s.token.balance(&donor_normal), 1_000);

    // Approve milestone: milestone_val * pledge_large overflows i128::MAX.
    // Instead of falling back to i128::MAX and draining donor_overflow,
    // it skips donor_overflow safely.
    s.client.approve_milestone(&id, &s.proof());

    // donor_overflow was safely skipped - balance untouched and pulled_amount is 0
    assert_eq!(s.token.balance(&donor_overflow), pledge_large);
    assert_eq!(s.client.get_pledge(&pid_overflow).unwrap().pulled_amount, 0);

    // donor_normal (5 tokens) does not overflow and was processed
    let pledge_normal = s.client.get_pledge(&pid_normal).unwrap();
    assert_eq!(
        s.token.balance(&donor_normal),
        1_000 - pledge_normal.pulled_amount
    );
}

#[test]
fn create_campaign_with_due_dates_rejects_due_dates_exceeding_deadline() {
    let s = Setup::new();
    let now = s.env.ledger().timestamp();
    let milestones = vec![&s.env, 300, 700];

    let count_before = s.client.campaign_count();

    // 1. Due date exceeding deadline (deadline = now + 10 * DAY, due_date = now + 15 * DAY) fails
    let late_dates = vec![&s.env, now + 5 * DAY, now + 15 * DAY];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://late-due"),
            &(now + 10 * DAY),
            &milestones,
            &late_dates,
        ),
        Err(Ok(Error::InvalidMilestones))
    );

    // 2. Due date = deadline + 1 fails
    let deadline = now + 10 * DAY;
    let over_by_one = vec![&s.env, now + 5 * DAY, deadline + 1];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://over-by-one"),
            &deadline,
            &milestones,
            &over_by_one,
        ),
        Err(Ok(Error::InvalidMilestones))
    );

    // 3. Due date equal to deadline succeeds
    let exact_deadline_dates = vec![&s.env, now + 5 * DAY, deadline];
    let res = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://exact-deadline-due"),
        &deadline,
        &milestones,
        &exact_deadline_dates,
    );
    assert_eq!(s.client.campaign_count(), count_before + 1);
    assert_eq!(s.client.milestone_due_dates(&res), exact_deadline_dates);
}

// ─── Issue #110: persistent keys are extended ────────────────────────────────

mod ttl {
    use super::*;
    use crate::storage::{INSTANCE_THRESHOLD, PERSISTENT_THRESHOLD};
    use crate::types::DataKey;
    use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};

    /// Remaining TTL, in ledgers, of a persistent key. Read through the test
    /// environment; it panics if the key does not exist.
    fn ttl(s: &Setup, key: &DataKey) -> u32 {
        s.env.as_contract(&s.client.address, || {
            s.env.storage().persistent().get_ttl(key)
        })
    }

    fn assert_extended(s: &Setup, key: DataKey) {
        let remaining = ttl(s, &key);
        assert!(
            remaining >= PERSISTENT_THRESHOLD,
            "{key:?} has only {remaining} ledgers left"
        );
    }

    /// A fresh entry in the test environment starts with a few thousand ledgers
    /// of TTL, so only an explicit `extend_ttl` on the write path can lift it
    /// to the threshold. Covers every persistent `DataKey` variant; the instance
    /// keys (`Admin`, `Token`, `CampaignCount`, `Paused`, `PlatformFee`,
    /// `Treasury`, `PendingAdmin`, `BondRequirement`, `BondWithdrawDelay`,
    /// `PledgeCount`) live in the instance entry, checked separately below.
    #[test]
    fn every_persistent_key_is_extended_by_the_call_that_writes_it() {
        let s = Setup::new();
        let id = s.campaign();
        assert_extended(&s, DataKey::Campaign(id));
        assert_extended(&s, DataKey::Verifier(s.verifier.clone()));
        assert_extended(&s, DataKey::AllowedToken(s.token.address.clone()));

        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &300);
        assert_extended(&s, DataKey::Contribution(id, donor.clone()));
        assert_extended(&s, DataKey::CampaignDonors(id));

        let sponsor = s.donor(1_000);
        s.client
            .deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500);
        assert_extended(&s, DataKey::SponsorPool(id, sponsor.clone()));

        let pledger = s.donor(500);
        let expiry = s.env.ledger().sequence() + 1_000;
        s.token.approve(&pledger, &s.client.address, &200, &expiry);
        let pledge_id = s.client.create_pledge(&pledger, &id, &200);
        assert_extended(&s, DataKey::Pledge(pledge_id));

        let bonded = s.bonded_verifier(1_000, 100);
        assert_extended(&s, DataKey::VerifierBond(bonded.clone()));
        s.client
            .set_verifier_profile(&bonded, &String::from_str(&s.env, "ipfs://profile"));
        assert_extended(&s, DataKey::VerifierProfile(bonded));

        let due_id = s.client.create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://due"),
            &(s.env.ledger().timestamp() + 30 * DAY),
            &vec![&s.env, 300, 300, 400],
            &vec![
                &s.env,
                s.env.ledger().timestamp() + 10 * DAY,
                s.env.ledger().timestamp() + 20 * DAY,
                s.env.ledger().timestamp() + 25 * DAY,
            ],
        );
        assert_extended(&s, DataKey::MilestoneDueDates(due_id));

        s.client.donate(&donor, &id, &700);
        s.client.approve_milestone(&id, &s.proof());
        assert_extended(&s, DataKey::MilestoneApprovals(id, 0));
    }

    /// Reading an entry also keeps it alive: advance the ledger until the
    /// entry is inside the extension window, read it through a view, and check
    /// the TTL went back up. Only calls that do not touch the token contract
    /// are made after the jump, because the token's own entries are not ours
    /// to extend.
    #[test]
    fn reads_of_verifier_contribution_and_allowlist_extend_their_entries() {
        let s = Setup::new();
        let id = s.campaign();
        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &300);

        s.env
            .ledger()
            .set_sequence_number(s.env.ledger().sequence() + 10 * 17_280);

        let verifier_key = DataKey::Verifier(s.verifier.clone());
        let contribution_key = DataKey::Contribution(id, donor.clone());
        let allowed_key = DataKey::AllowedToken(s.token.address.clone());
        for key in [&verifier_key, &contribution_key, &allowed_key] {
            assert!(ttl(&s, key) < PERSISTENT_THRESHOLD, "{key:?} should be due");
        }

        assert!(s.client.is_verifier(&s.verifier));
        assert_eq!(s.client.contribution_of(&id, &donor), 300);
        assert!(s.client.is_token_allowed(&s.token.address));

        assert_extended(&s, verifier_key);
        assert_extended(&s, contribution_key);
        assert_extended(&s, allowed_key);
    }

    /// The instance entry (admin, token, counters, fees) is extended by every
    /// campaign call, not only by the constructor and `create_campaign`.
    #[test]
    fn campaign_calls_extend_the_instance_entry() {
        let s = Setup::new();
        let id = s.campaign();

        s.env
            .ledger()
            .set_sequence_number(s.env.ledger().sequence() + 5 * 17_280);
        let before = s
            .env
            .as_contract(&s.client.address, || s.env.storage().instance().get_ttl());
        assert!(before < INSTANCE_THRESHOLD);

        s.client.get_campaign(&id);
        let after = s
            .env
            .as_contract(&s.client.address, || s.env.storage().instance().get_ttl());
        assert!(
            after >= INSTANCE_THRESHOLD,
            "instance left with {after} ledgers"
        );
    }
}

// ─── Issue #116: split payouts ───────────────────────────────────────────────

mod split_payouts {
    use super::*;
    use crate::Beneficiary;

    fn split_campaign(s: &Setup, shares: &[(Address, u32)]) -> u64 {
        let mut list = vec![&s.env];
        for (to, bps) in shares {
            list.push_back(Beneficiary {
                to: to.clone(),
                bps: *bps,
            });
        }
        s.client.create_campaign_beneficiaries(
            &s.creator,
            &list,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://split"),
            &(s.env.ledger().timestamp() + 30 * DAY),
            &vec![&s.env, 300, 300, 400],
        )
    }

    #[test]
    fn milestone_payout_is_split_and_rounding_dust_goes_to_the_first_beneficiary() {
        let s = Setup::new();
        let (a, b, c) = (
            Address::generate(&s.env),
            Address::generate(&s.env),
            Address::generate(&s.env),
        );
        let id = split_campaign(
            &s,
            &[(a.clone(), 3_333), (b.clone(), 3_333), (c.clone(), 3_334)],
        );
        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &1_000);

        s.client.approve_milestone(&id, &s.proof());
        assert!(!s.env.events().all().events().is_empty());

        // 300 splits into 99, 99 and 100; the 2 left over go to the first.
        assert_eq!(s.token.balance(&a), 101);
        assert_eq!(s.token.balance(&b), 99);
        assert_eq!(s.token.balance(&c), 100);
        assert_eq!(
            s.token.balance(&a) + s.token.balance(&b) + s.token.balance(&c),
            300
        );
        assert_eq!(s.client.get_beneficiaries(&id).len(), 3);
    }

    #[test]
    fn emergency_advance_is_split_across_beneficiaries() {
        let s = Setup::new();
        let (a, b, c) = (
            Address::generate(&s.env),
            Address::generate(&s.env),
            Address::generate(&s.env),
        );
        let id = split_campaign(
            &s,
            &[(a.clone(), 3_333), (b.clone(), 3_333), (c.clone(), 3_334)],
        );
        let donor = s.donor(500);
        s.client.donate(&donor, &id, &300);

        // 20% of the 1000 goal is 200: 66 + 66 + 66 plus 2 of dust to the first.
        assert_eq!(s.client.emergency_fast_track(&id), 200);
        assert_eq!(s.token.balance(&a), 68);
        assert_eq!(s.token.balance(&b), 66);
        assert_eq!(s.token.balance(&c), 66);
    }

    #[test]
    fn shares_must_sum_to_10_000_and_addresses_must_be_distinct() {
        let s = Setup::new();
        let (a, b) = (Address::generate(&s.env), Address::generate(&s.env));
        let try_create = |s: &Setup, shares: &[(Address, u32)]| {
            let mut list = vec![&s.env];
            for (to, bps) in shares {
                list.push_back(Beneficiary {
                    to: to.clone(),
                    bps: *bps,
                });
            }
            s.client.try_create_campaign_beneficiaries(
                &s.creator,
                &list,
                &s.verifier,
                &s.token.address,
                &CampaignKind::Emergency,
                &String::from_str(&s.env, "ipfs://bad"),
                &(s.env.ledger().timestamp() + 30 * DAY),
                &vec![&s.env, 300, 300, 400],
            )
        };
        assert!(try_create(&s, &[(a.clone(), 10_000)]).is_ok());
        assert_eq!(
            try_create(&s, &[(a.clone(), 5_000), (b.clone(), 4_000)]),
            Err(Ok(Error::InvalidBeneficiaries))
        );
        assert_eq!(
            try_create(&s, &[(a.clone(), 6_000), (b.clone(), 5_000)]),
            Err(Ok(Error::InvalidBeneficiaries))
        );
        assert_eq!(
            try_create(&s, &[(a.clone(), 5_000), (a.clone(), 5_000)]),
            Err(Ok(Error::InvalidBeneficiaries))
        );
        assert_eq!(
            try_create(&s, &[(a.clone(), 0), (b.clone(), 10_000)]),
            Err(Ok(Error::InvalidBeneficiaries))
        );
        assert_eq!(try_create(&s, &[]), Err(Ok(Error::InvalidBeneficiaries)));
    }

    #[test]
    fn the_verifier_cannot_be_a_beneficiary() {
        let s = Setup::new();
        let other = Address::generate(&s.env);
        let mut list = vec![&s.env];
        list.push_back(Beneficiary {
            to: other,
            bps: 5_000,
        });
        list.push_back(Beneficiary {
            to: s.verifier.clone(),
            bps: 5_000,
        });
        assert_eq!(
            s.client.try_create_campaign_beneficiaries(
                &s.creator,
                &list,
                &s.verifier,
                &s.token.address,
                &CampaignKind::Emergency,
                &String::from_str(&s.env, "ipfs://conflict"),
                &(s.env.ledger().timestamp() + 30 * DAY),
                &vec![&s.env, 300, 300, 400],
            ),
            Err(Ok(Error::ConflictOfInterest))
        );
    }

    #[test]
    fn a_single_beneficiary_campaign_still_pays_the_beneficiary_directly() {
        let s = Setup::new();
        let id = s.campaign();
        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &1_000);
        s.client.approve_milestone(&id, &s.proof());

        assert_eq!(s.token.balance(&s.beneficiary), 300);
        let single = s.client.get_beneficiaries(&id);
        assert_eq!(single.len(), 1);
        assert_eq!(single.get(0).unwrap().bps, 10_000);
    }
}

// ─── Issue #118: schema versioning ───────────────────────────────────────────

mod schema {
    use super::*;
    use crate::SCHEMA_VERSION;
    use crate::types::DataKey;

    /// Rewrites the stored state as an older build (before versioning) left it.
    fn downgrade_to_unversioned(s: &Setup) {
        s.env.as_contract(&s.client.address, || {
            s.env.storage().instance().remove(&DataKey::SchemaVersion);
        });
    }

    #[test]
    fn fresh_deploys_start_at_the_current_version() {
        let s = Setup::new();
        assert_eq!(s.client.schema_version(), SCHEMA_VERSION);
        let id = s.campaign();
        let donor = s.donor(100);
        s.client.donate(&donor, &id, &100);
    }

    #[test]
    fn calls_fail_while_a_migration_is_pending_and_migrate_fixes_it() {
        let s = Setup::new();
        let id = s.campaign();
        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &100);

        downgrade_to_unversioned(&s);
        assert_eq!(s.client.schema_version(), 0);

        // State-changing entry points refuse to run, reads still work.
        assert_eq!(
            s.client.try_donate(&donor, &id, &100),
            Err(Ok(Error::MigrationPending))
        );
        assert_eq!(
            s.client.try_create_campaign(
                &s.creator,
                &s.beneficiary,
                &s.verifier,
                &s.token.address,
                &CampaignKind::Emergency,
                &String::from_str(&s.env, "ipfs://pending"),
                &(s.env.ledger().timestamp() + 30 * DAY),
                &vec![&s.env, 300, 300, 400],
            ),
            Err(Ok(Error::MigrationPending))
        );
        assert_eq!(s.client.get_campaign(&id).raised, 100);

        // The old data survives the migration untouched.
        assert_eq!(s.client.migrate(), SCHEMA_VERSION);
        assert!(!s.env.events().all().events().is_empty(), "Migrated event");
        assert_eq!(s.client.schema_version(), SCHEMA_VERSION);
        assert_eq!(s.client.get_campaign(&id).raised, 100);
        assert_eq!(s.client.donate(&donor, &id, &100), 100);
        assert_eq!(s.client.get_campaign(&id).raised, 200);
    }

    #[test]
    fn migrate_is_idempotent_and_admin_only() {
        let s = Setup::new();
        let id = s.campaign();
        let before = s.client.get_campaign(&id);

        assert_eq!(s.client.migrate(), SCHEMA_VERSION);
        assert_eq!(s.client.migrate(), SCHEMA_VERSION);
        assert!(!s.env.events().all().events().is_empty(), "Migrated event");
        assert_eq!(s.client.get_campaign(&id), before);

        downgrade_to_unversioned(&s);
        assert!(s.client.mock_auths(&[]).try_migrate().is_err());
        assert_eq!(s.client.schema_version(), 0);
    }
}

// ─── Issue #119: verifier track record ───────────────────────────────────────

mod verifier_track_record {
    use super::*;

    #[test]
    fn assigned_approved_and_completed_counters_follow_a_campaign() {
        let s = Setup::new();
        let empty = s.client.verifier_stats(&s.verifier);
        assert_eq!(empty.campaigns_assigned, 0);

        let id = s.campaign();
        assert_eq!(s.client.verifier_stats(&s.verifier).campaigns_assigned, 1);

        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &1_000);
        s.client.approve_milestone(&id, &s.proof());
        s.client.approve_milestone(&id, &s.proof());
        let mid = s.client.verifier_stats(&s.verifier);
        assert_eq!(mid.milestones_approved, 2);
        assert_eq!(mid.campaigns_completed, 0);

        s.client.approve_milestone(&id, &s.proof());
        let done = s.client.verifier_stats(&s.verifier);
        assert_eq!(done.milestones_approved, 3);
        assert_eq!(done.campaigns_completed, 1);
        assert_eq!(done.campaigns_cancelled, 0);
    }

    #[test]
    fn cancelling_counts_against_the_assigned_verifier() {
        let s = Setup::new();
        let id = s.campaign();
        s.client.cancel_campaign(&s.creator, &id);
        let second = s.campaign();
        s.client.cancel_campaign_with_reason(
            &s.creator,
            &second,
            &String::from_str(&s.env, "ipfs://why"),
        );

        let stats = s.client.verifier_stats(&s.verifier);
        assert_eq!(stats.campaigns_assigned, 2);
        assert_eq!(stats.campaigns_cancelled, 2);
        assert_eq!(stats.campaigns_completed, 0);
    }

    #[test]
    fn a_reassigned_campaign_counts_for_the_verifier_who_held_it() {
        let s = Setup::new();
        let id = s.campaign();
        let donor = s.donor(1_000);
        s.client.donate(&donor, &id, &1_000);
        s.client.approve_milestone(&id, &s.proof());

        let second = Address::generate(&s.env);
        s.client.add_verifier(&second);
        s.client.reassign_verifier(&id, &second);

        // The original verifier keeps what they did; the new one is assigned.
        let first = s.client.verifier_stats(&s.verifier);
        assert_eq!(first.campaigns_assigned, 1);
        assert_eq!(first.milestones_approved, 1);
        assert_eq!(s.client.verifier_stats(&second).campaigns_assigned, 1);

        // Events after the handover count for the new verifier only.
        s.client.cancel_campaign(&s.creator, &id);
        assert_eq!(s.client.verifier_stats(&second).campaigns_cancelled, 1);
        assert_eq!(s.client.verifier_stats(&s.verifier).campaigns_cancelled, 0);
    }

    #[test]
    fn slashes_add_up_in_total_slashed() {
        let s = Setup::new();
        let v = s.bonded_verifier(1_000, 400);
        let id = s.campaign();

        s.client.slash_verifier(&v, &id, &150);
        s.client.slash_verifier(&v, &id, &50);
        assert_eq!(s.client.verifier_stats(&v).total_slashed, 200);
        assert_eq!(s.client.verifier_stats(&s.verifier).total_slashed, 0);
    }
}
