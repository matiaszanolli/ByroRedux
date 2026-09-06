# #3869: TD3-2026-09-05-05: `legacy_pbr_translation_tests.rs`'s module doc still names the deleted `Material::classify_pbr` as a live sharing partner — the site #1624's own SIBLING completeness check was meant to catch

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-05) via `/audit-publish`, 2026-09-05. Labels: `low,nifal,nif-parser,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3869 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-05), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `crates/nif/src/import/material/legacy_pbr_translation_tests.rs:7-8`
- **Status**: Same class as CLOSED #1624 / #1522 / #1321 — **new site**
- **Effort**: trivial (≤30 min)
- **Age**: the file was created 2026-05-25 (`7fe85158`). #1624's fix landed 2026-06-15 (`eb9f6983`), three weeks later, and repaired only the sibling doc in `import/material/mod.rs`. **~3.5 months stale, and it was already in the tree when the sweep that should have found it ran.**
- **Description**: The module docstring reads:
  > *"The classifier itself **is shared with `Material::classify_pbr`** via `byroredux_core::ecs::components::material::classify_pbr_keyword`, so the heavy keyword-arm coverage lives next to that function in the core crate."*

  `Material::classify_pbr` was deleted in the NIFAL refactor — PBR resolves once at the parse-time `translate_material` boundary. The present-tense "is shared with" asserts a live render-time consumer that does not exist. (The sentence does then name the real free function, which is why this is LOW rather than the outright-misdirection of the earlier sites.)
- **Evidence**:
  ```
  $ grep -rn "fn classify_pbr\b" --include='*.rs' .        # zero hits
  $ grep -n "classify_pbr" crates/core/src/ecs/components/material.rs
  828: /// `Material::classify_pbr` (the per-draw fallback that was removed in
  1235: ///  glossiness-fallback in the (deleted per-draw) `classify_pbr`
  1417: /// the hard-coded lists in the (deleted) `Material::classify_pbr`
  1486: /// fields, the way the deleted `Material::classify_pbr` used to
  1008: pub fn classify_pbr_keyword(inputs: PbrClassifierInputs<'_>) -> PbrMaterial {
  ```
  The canonical file the skill points at is **clean** — every mention there is explicitly `(deleted)`. The surviving live-framing is only in this NIF-side test module. `crates/spt/src/import/mod.rs` also mentions `classify_pbr_keyword`, correctly, by its live name.
- **Impact**: This is the fourth site of a defect class already closed three times (#1321, #1522, #1624). Its consequence is a NIFAL-invariant misread: a reader concludes a render-time PBR classifier still exists, contradicting `docs/engine/nifal.md`'s "resolve-once at the translate boundary / no-render-time-fallback" rule — the rule whose violation `_audit-severity.md` scores HIGH. `feedback_audit_findings.md` records that ~5 of 30 findings in a past sweep had stale premises; a doc asserting a deleted classifier is live is precisely how such a premise is manufactured.
- **Related**: #1624 (whose completeness check read *"SIBLING: No other doc names the deleted `Material::classify_pbr` as live"* — this file falsifies that check), #1522, #1321.
- **Suggested Fix**: Rewrite to *"The classifier itself is the free function `byroredux_core::ecs::components::material::classify_pbr_keyword`, shared by the parser-side and canonical-translation paths; the render-time `Material::classify_pbr` it once mirrored was removed in the NIFAL refactor."* While there, run `grep -rn "Material::classify_pbr"` across the whole tree and confirm every remaining mention carries "deleted"/"removed" — this is the recurrence that keeps escaping single-file fixes.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
