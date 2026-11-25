// Resource ceilings for the main calls, read from the host budget.
//
// | call               | CPU instructions | memory bytes |
// |--------------------|------------------|--------------|
// | `create_campaign`  | 25,000,000       | 10,485,760   |
// | `donate`           | 25,000,000       | 10,485,760   |
// | `approve_milestone`| 25,000,000       | 10,485,760   |
// | `refund`           | 25,000,000       | 10,485,760   |
// | `apply_matching`   | 25,000,000       | 10,485,760   |
//
// The ceilings are a quarter of the network's per-transaction limits (100M
// instructions, 40 MiB), so a call that grows towards a mainnet limit fails
// here first. They are deliberately loose until each call's measured value is
// recorded: run `cargo test budget -- --nocapture`, which prints the measured
// numbers, then set each ceiling about 25 percent above its number and write
// the measured value next to it in the table above.
use super::*;

struct Ceiling {
    call: &'static str,
    cpu: u64,
    memory: u64,
}

const CEILINGS: [Ceiling; 5] = [
    Ceiling {
        call: "create_campaign",
        cpu: 25_000_000,
        memory: 10_485_760,
    },
    Ceiling {
        call: "donate",
        cpu: 25_000_000,
        memory: 10_485_760,
    },
    Ceiling {
        call: "approve_milestone",
        cpu: 25_000_000,
        memory: 10_485_760,
    },
    Ceiling {
        call: "refund",
        cpu: 25_000_000,
        memory: 10_485_760,
    },
    Ceiling {
        call: "apply_matching",
        cpu: 25_000_000,
        memory: 10_485_760,
    },
];

/// Resets the host budget so the next contract call is measured on its own.
fn start(s: &Setup) {
    s.env.cost_estimate().budget().reset_default();
}

/// Reads what the call since `start` used and fails, naming the call and
/// the overshoot, if it is above its ceiling.
fn check(s: &Setup, call: &str) {
    let ceiling = CEILINGS.iter().find(|c| c.call == call).unwrap();
    let budget = s.env.cost_estimate().budget();
    let cpu = budget.cpu_instruction_cost();
    let memory = budget.memory_bytes_cost();
    std::println!("budget {call}: {cpu} CPU instructions, {memory} memory bytes");
    assert!(
        cpu <= ceiling.cpu,
        "{call} used {cpu} CPU instructions, over its ceiling of {} by {}",
        ceiling.cpu,
        cpu - ceiling.cpu
    );
    assert!(
        memory <= ceiling.memory,
        "{call} used {memory} memory bytes, over its ceiling of {} by {}",
        ceiling.memory,
        memory - ceiling.memory
    );
}

#[test]
fn main_calls_stay_within_their_resource_ceilings() {
    let s = Setup::new();
    let donor = s.donor(5_000);
    let sponsor = s.donor(5_000_000);

    start(&s);
    let id = s.campaign();
    check(&s, "create_campaign");

    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000);

    start(&s);
    s.client.donate(&donor, &id, &400);
    check(&s, "donate");

    start(&s);
    s.client.apply_matching(&sponsor, &id, &donor, &400);
    check(&s, "apply_matching");

    start(&s);
    s.client.approve_milestone(&id, &s.proof());
    check(&s, "approve_milestone");

    s.client.cancel_campaign(&s.creator, &id);
    start(&s);
    s.client.refund(&donor, &id);
    check(&s, "refund");
}

#[test]
fn every_call_in_the_ceiling_table_is_measured() {
    let measured = [
        "create_campaign",
        "donate",
        "approve_milestone",
        "refund",
        "apply_matching",
    ];
    for ceiling in CEILINGS.iter() {
        assert!(
            measured.contains(&ceiling.call),
            "{} has no measurement",
            ceiling.call
        );
    }
    assert_eq!(CEILINGS.len(), measured.len());
}
