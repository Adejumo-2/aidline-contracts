use super::*;

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
