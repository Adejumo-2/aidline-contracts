use super::*;

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
