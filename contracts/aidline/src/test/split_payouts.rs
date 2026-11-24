// ─── Issue #116: split payouts ───────────────────────────────────────────────

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
    s.client.create_campaign_with_beneficiaries(
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
    let id = split_campaign(&s, &[(a.clone(), 3_333), (b.clone(), 3_333), (c.clone(), 3_334)]);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);

    s.client.approve_milestone(&id, &s.proof());

    // 300 splits into 99, 99 and 100; the 2 left over go to the first.
    assert_eq!(s.token.balance(&a), 101);
    assert_eq!(s.token.balance(&b), 99);
    assert_eq!(s.token.balance(&c), 100);
    assert_eq!(
        s.token.balance(&a) + s.token.balance(&b) + s.token.balance(&c),
        300
    );
    assert!(!s.env.events().all().events().is_empty());
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
    let id = split_campaign(&s, &[(a.clone(), 3_333), (b.clone(), 3_333), (c.clone(), 3_334)]);
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
        s.client.try_create_campaign_with_beneficiaries(
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
        s.client.try_create_campaign_with_beneficiaries(
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
