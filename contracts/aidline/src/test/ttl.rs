// ─── Issue #110: persistent keys are extended ────────────────────────────────

use super::*;
use crate::storage::{INSTANCE_THRESHOLD, PERSISTENT_THRESHOLD};
use crate::types::DataKey;
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};

/// Remaining TTL, in ledgers, of a persistent key. Read through the test
/// environment; it panics if the key does not exist.
fn ttl(s: &Setup, key: &DataKey) -> u32 {
    s.env
        .as_contract(&s.client.address, || s.env.storage().persistent().get_ttl(key))
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
    s.client.deposit_sponsor_pool(&sponsor, &id, &5_000, &500, &500);
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
    assert!(after >= INSTANCE_THRESHOLD, "instance left with {after} ledgers");
}
