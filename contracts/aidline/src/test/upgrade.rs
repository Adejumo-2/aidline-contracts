use super::*;

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

    pub fn contribution_of(env: Env, campaign_id: u64, donor: Address) -> i128 {
        crate::storage::contribution(&env, campaign_id, &donor)
    }

    pub fn get_verifier_bond(env: Env, verifier: Address) -> Option<crate::VerifierBond> {
        crate::storage::verifier_bond(&env, &verifier)
    }
}

#[test]
fn admin_can_upgrade_contract_and_preserves_state() {
    let s = Setup::new();
    let bonded_verifier = s.donor(500);
    s.client.register_with_bond(&bonded_verifier, &200);

    let id = s.campaign();
    let donor = s.donor(500);
    s.client.donate(&donor, &id, &300);

    // Upload the new contract code
    let new_wasm_hash = s.env.upload(MockUpgradedAidline);

    // Admin upgrades the contract
    s.client.upgrade(&new_wasm_hash);
    assert!(
        !s.env.events().all().events().is_empty(),
        "ContractUpgraded event should be emitted"
    );

    // Verify upgrade took effect at the same contract address
    let upgraded_client = MockUpgradedAidlineClient::new(&s.env, &s.client.address);
    assert_eq!(upgraded_client.version(), 2);

    // State (campaign, contributions, and verifier bonds) is preserved across upgrade
    let c = upgraded_client.get_campaign(&id);
    assert_eq!(c.id, id);
    assert_eq!(c.goal, 1000);
    assert_eq!(c.raised, 300);
    assert_eq!(upgraded_client.contribution_of(&id, &donor), 300);
    let bond = upgraded_client
        .get_verifier_bond(&bonded_verifier)
        .expect("bond should persist across upgrade");
    assert_eq!(bond.amount, 200);
    assert_eq!(bond.remaining, 200);
}

#[test]
fn non_admin_cannot_upgrade_contract() {
    let s = Setup::new();
    let new_wasm_hash = s.env.upload(MockUpgradedAidline);

    let res = s.client.mock_auths(&[]).try_upgrade(&new_wasm_hash);
    assert!(res.is_err());
}
