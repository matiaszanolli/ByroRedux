# #3863: TD2-2026-09-05-06: `extensions.rs` repeats the guest-entry snapshot prologue ten times and the 11-field `DeliveryCommitContext` literal fourteen times

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-06) via `/audit-publish`, 2026-09-05. Labels: `low,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3863 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-06), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**: `byroredux/src/extensions.rs` — the prologue/epilogue pair inside
  `dispatch_console_command`, `invoke_owned_papyrus_provider`, and the
  `dispatch_*_inner` family (activate, cell-load, equipment, input-action,
  session, pending-custom-event, and two more), plus
  `struct DeliveryCommitContext<'a>` / `fn apply_delivery_result` at the bottom
  of the file
- **Status**: NEW
- **Description**: Every path that enters a sandboxed guest repeats the same
  block verbatim:
  `let principal = hosted.instance.principal().id().clone();` → read
  `self.principal_storage.values(&principal)` → `set_principal_storage_snapshot`
  → read `self.legacy_containers.get(&principal)` →
  `set_legacy_container_snapshot` → invoke the guest → `apply_delivery_result(…,
  DeliveryCommitContext { state, principal_storage, legacy_containers,
  pending_custom_events, pending_setting_writes, pending_actor_value_writes,
  pending_package_evaluations, pending_animation_commands,
  pending_reputation_writes, diagnostics, stats })`. The commit context is
  already a named struct and `apply_delivery_result` is already a free function
  — what was never factored is the *construction*, so the 11 `&mut self.<field>`
  borrows are typed out at every site.
- **Evidence**: 10 `set_principal_storage_snapshot` production call sites, each
  paired 1:1 with a `set_legacy_container_snapshot` (a perfectly symmetric
  10/10 in production; the extra hits are in the test module past ~line 6000).
  14 `DeliveryCommitContext { … }` literals × 11 fields ≈ 154 lines of pure
  borrow plumbing. `byroredux/src/extensions.rs` is ~5920 production LOC and is
  the largest file in the workspace — this is one of the reasons.
- **Impact**: A twelfth pending-command queue (the file already has six:
  custom events, setting writes, actor-value writes, package evaluations,
  animation commands, reputation writes — and the SDK surface is still growing)
  means editing fourteen call sites, and a site that forgets a field does not
  fail to compile if the field is later given a default. Named in the SKILL's
  own "young crates that have not yet seen a debt sweep" list.
- **Related**: `crates/mod-runtime` / `crates/sdk` (the crates this file
  adapts); Dim 1's `extensions.rs` finding (same file, different axis — Dim 1
  owns the file split, this owns the repeated block).
- **Suggested Fix**: Two moves inside `byroredux/src/extensions.rs`. (1) Group
  the nine non-`components` owned fields into a `struct DeliveryState` field on
  the host, so `let (components, delivery) = (&mut self.components, &mut self.delivery);`
  splits the borrow cleanly and `DeliveryCommitContext::new(delivery, &mut stats)`
  becomes one call. (2) Extract the prologue as a free
  `fn enter_guest(hosted: &mut HostedComponent, delivery: &DeliveryState) -> PrincipalId`
  — a free function, not a `&mut self` method, so the `hosted` borrow does not
  conflict. Each dispatch site then reads: bind entity, `enter_guest`, invoke,
  `apply_delivery_result`.
- **Effort**: medium (≤1 day)

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
