# #4081 — ESM-2026-09-09-D7-06

`LegacyFormId::is_null` tests the 24-bit standard local id for ESL and ESH forms, so a null light-master reference resolves to a valid-looking `FormIdPair`

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4081 --json state`).

---

- **Severity**: LOW
- **Dimension**: ESM→ECS Handoff (Redux-native / legacy bridge, rot + latent logic)
- **Record / Sub-record**: —
- **Location**: `crates/plugin/src/legacy/mod.rs:69-72`, consumed at `:173-176`
- **Status**: NEW — re-verified against **code** at HEAD (source report
  `AUDIT_ESM_2026-08-13.md` ESM-D7-08 pre-dates 2026-06-07); no issue exists.
- **Description**: `is_null` returns `local_id() == 0`, where `local_id` masks the
  bottom **24** bits. An ESL form's object id is the bottom **12** bits
  (`esl_local`), and an ESH form's is the bottom **16** (`esh_local`). A null ESL
  reference such as `0xFE00_A000` has `local_id() == 0x00A000 != 0`, so `is_null`
  is `false`, `resolve` proceeds past its null gate, and returns
  `Some(FormIdPair { plugin: <esl at sub-index 0x00A>, local: LocalFormId(0) })`.
- **Evidence**:
  ```rust
  // crates/plugin/src/legacy/mod.rs:69-72
      /// Returns true if this is a null/invalid form (local_id == 0).
      pub fn is_null(&self) -> bool {
          self.local_id() == 0
      }
  ```
  ```rust
  // crates/plugin/src/legacy/mod.rs:173-176
      pub fn resolve(&self, legacy: LegacyFormId) -> Option<FormIdPair> {
          if legacy.is_save_generated() || legacy.is_null() {
              return None;
          }
  ```
  The ESL and ESH branches at `:178-192` use `esl_local()` / `esh_local()`, so the
  module already knows the correct widths — only the null gate does not.
- **Impact**: none today; the module is `pub(crate)` scaffolding with no caller
  (`crates/plugin/src/lib.rs:30`), and this is reported as **rot in tested code**,
  not as a live bug — per the checklist's rot-only scope for this tier, and
  explicitly not as an "unused" complaint. It becomes a real null-deref-shaped bug
  the moment the ESL-aware consumer lands, which is the stated purpose of the module.
- **Related**: `AUDIT_ESM_2026-08-13.md` ESM-D7-08; #1322 (the `pub(crate)` rationale).
- **Suggested Fix**: dispatch `is_null` on the slot kind —
  `if is_esl() { esl_local() == 0 } else if is_esh() { esh_local() == 0 } else { local_id() == 0 }` —
  and add the two negative cases to the existing `is_*` test block.

---
