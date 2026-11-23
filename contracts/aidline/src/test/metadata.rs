use super::*;

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
