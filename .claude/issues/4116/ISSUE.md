# SCR-D6-2026-09-11-01: pinned ActivateEvent consumer-order test omits the fourth real consumer, mg07_on_activate_dispatch

URL: https://github.com/matiaszanolli/ByroRedux/issues/4116
Labels: bug, low, scripting, test-gap

- **Severity**: LOW (test-coverage gap only; today's registration order is correct — this is exactly the invariant that regressed once already, as SCR-D6-2026-09-06-01, without any test catching it for 187 commits)
- **Dimension**: Scripting Runtime Systems — Lifecycle, Stage & Lock Ordering (audit-scripting)
- **Location**: `byroredux/src/boot/schedule/mod.rs:75-79` (the consumer list in `activation_flush_is_scheduled_before_every_activate_event_consumer`); `byroredux/src/boot/schedule/update.rs:75-77` (`mg07_on_activate_dispatch` fn), `:315` (registration)
- **Status**: NEW

**Description**

The regression test added to close SCR-D6-2026-09-06-01 asserts `fragment_activation_flush_system` runs before three named `ActivateEvent` consumers (`rumble_on_activate_dispatch`, `quest_advance_dispatch`, `two_state_activator_system`). The prior HIGH named a *fourth* real consumer, `mg07_on_activate_dispatch` (confirmed reading `ActivateEvent` via the standard two-phase collect/drop pattern), registered at `update.rs:315` — after the flush at `:172`, so today's order is correct — but the test doesn't cover it. A future reorder (the exact edit class the `#3739`/`#3855` boot-splitting refactors already warn about) could silently reintroduce a one-consumer version of the original HIGH, undetected.

**Evidence**

`grep -n "mg07_on_activate_dispatch" byroredux/src/boot/schedule/mod.rs` → zero matches inside the pinned test.

**Impact**

None today; latent regression risk scoped to exactly the invariant this crate has already been burned by once.

**Related**

SCR-D6-2026-09-06-01 (the HIGH this test was added to guard, CLOSED); `#3936` (the fix commit)

**Suggested Fix**

Add the `mg07_on_activate_dispatch` registration-site needle to the consumer array at `mod.rs:75-79`.

## Completeness Checks
- [ ] **TESTS**: The added needle actually fails on a deliberate reorder before landing (verify the test is load-bearing, not vacuous)

Source: `docs/audits/AUDIT_SCRIPTING_2026-09-11.md`
