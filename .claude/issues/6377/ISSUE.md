# TD3-2026-10-25-06: #6237 removed `RigidBodyData` from the save registry (v116), but three places still describe it as a registered, preserved column

Labels: low, save-load, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6377
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW
- **Dimension**: 3
- **Effort**: trivial
- **Location**:
  - `docs/engine/save-load-roundtrip.md:226-231` ("a curated mutable-column set of 24 entries (... `CharacterController`, `RigidBodyData` registered *preserving* so its `serde(skip)` `part_surface_materials` survives the overlay, #5686 ...)"). `MUTABLE_DELTA_COLUMNS` (`byroredux/src/save_io/registry.rs`) has 30 entries and no `RigidBodyData`.
  - `crates/core/src/ecs/components/collision.rs:148-152` ("The live-load overlay registers this component *preserving* -- the apply hook (`keep_reimported_part_surface_materials`, #5686)"). The fn was deleted in `6765e60eb` (2026-10-21, #6237); `snapshot.rs:582` records "the `RigidBodyData` column is gone". The `part_surface_materials` doc also still reasons about the save fingerprint.
  - `docs/engine/save-load-roundtrip.md:18` (a historical "registered component count doubled (... `RigidBodyData` ...)" aside) is dated and fine. `crates/save/src/registry.rs:117` uses `RigidBodyData::part_surface_materials` as the *example* for `register_component_preserving` (still valid code, but `Transform` is now the only user, `:567`).
- **Fix**: Update the count to 30, drop the `RigidBodyData` clause, and replace the `collision.rs` sentence with "not saved (the column is gone, #6237); re-derived at import". Update the `registry.rs:117` example to `Transform`/`keep_live_translation`.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-06`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
