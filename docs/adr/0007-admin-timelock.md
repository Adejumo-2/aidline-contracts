# 0007: Timelock for sensitive admin actions

Status: Accepted

## Context

A single compromised admin key can reassign a campaign's verifier, slash a bond and change bond rules instantly. Donors and verifiers have no time to react before the change takes effect.

## Decision

Add a queue and execute pattern:

1. `queue_action(action)` (admin only) stores a `TimelockedAction` with `eta = now + TIMELOCK_DELAY` and emits `tl_queue`.
2. `execute_action(action_id)` (anyone) runs the action once `now >= eta` and no later than `eta + TIMELOCK_GRACE_PERIOD`. After that it fails with `ActionExpired`.
3. `cancel_action(action_id)` (admin only) removes a queued action and emits `tl_cancel`. Execution emits `tl_exec`.

Queueable actions: `ReassignVerifier`, `SlashVerifier`, `SetBondRequirement`, `SetBondWithdrawDelay`. Each runs the same code as its direct admin entry point.

Delay is 2 days and the grace period is 7 days. The delay is long enough for a donor or verifier to notice a queued action and for the admin to cancel a mistaken one.

## Consequences

- Observers can watch `tl_queue` events and react before a change lands.
- The direct entry points (`reassign_verifier`, `slash_verifier`, `set_bond_requirement`, `set_bond_withdraw_delay`) are not removed or gated by this change, so a compromised admin key can still call them. Gating them is a follow-up that needs a migration note for existing integrations.
- Expired actions are not deleted automatically. The admin can cancel them to clean up storage.

## Alternatives considered

- **Gate the direct functions immediately:** closes the bypass but breaks every existing caller and test in one step.
- **Per-action delays:** more flexible but harder to reason about and to audit.
