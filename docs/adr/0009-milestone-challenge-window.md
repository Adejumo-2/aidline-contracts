# 0009: Challenge window before a milestone is paid

Status: Accepted

## Context

Approval pays the beneficiary in the same transaction. If a verifier key is compromised, or a verifier approves work that was not done, nobody can stop the payment.

## Decision

A campaign can be created with a challenge window (`create_campaign_challenge_window`, at most 14 days). With a window above zero, an approval that reaches the verifier threshold becomes a **proposal** instead of a payout:

1. `approve_milestone`, or `propose_milestone_approval` for multisig verifiers, records the proposal and emits `proposed` with the time the window ends. No funds move and the milestone stays pending. Only one proposal can wait at a time.
2. During the window the admin can `block_milestone` alone. Donors can also block: each donor's vote counts for their contribution, once per proposal, and the proposal is blocked when the votes reach 30% of `raised`. A blocked approval is removed, its approvals are cleared and a `blocked` event is emitted, so the verifier can propose it again.
3. After the window anyone can `finalize_milestone`. That emits `finalized`, then pays out exactly as an immediate approval would (pledges pulled, funding checked, fee and beneficiary paid, `released` event).

A window of `0` is the old behaviour: approval pays immediately and nothing changes.

## Consequences

- A compromised verifier can no longer drain a campaign in one transaction. The admin or a block of donors has the length of the window to react.
- The window delays honest payouts by the same amount, so campaigns choose it at creation and it is capped.
- The 30% quorum is lower than the 50% needed to cancel a stalled campaign on purpose: stopping one payment is a smaller step than ending the campaign.
- Finalizing can fail with `MilestoneNotFunded` when escrow does not cover the milestone; the proposal stays and can be finalized later.
- A proposal that is still waiting when a campaign is cancelled or expires can never be finalized, because finalizing needs an open campaign.

## Alternatives considered

- **A flat delay for every campaign:** hurts urgent disaster relief where speed matters.
- **Only the admin can block:** puts the whole safeguard on one key, which is the risk this decision is about.
- **Make block votes permanent across proposals:** a donor could not react to a new, different proposal.
