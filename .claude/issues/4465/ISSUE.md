# P3-SAVE: SAVE-D2-01 regression from P3 reference state: `serde(default)` on `ReferenceState.picked_up` + stale shape baseline — bump `FORMAT_MAJOR`, remove the default, regenerate the baseline

- **Labels**: medium,bug,save-load
- **Filed**: 2026-09-19, follow-up to the #4458 fix session (post-/audit-character)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4465

---

Discovered 2026-09-19 while fixing #4458: the two save-format guards in `byroredux/src/save_io/serde_default_guard_tests.rs` fail red (confirmed pre-existing on compile-repaired pristine HEAD). Both were introduced by the P3 persistent-reference-state work landing without following the save-format discipline its own guards enforce.

**Evidence**

- `serde_default_on_saved_struct_requires_format_major_bump` (serde_default_guard_tests.rs:733) — SAVE-D2-01 (#1714): `#[serde(default)]` on the serialized `ReferenceState.picked_up` field (`byroredux/src/cell_loader/reference_state.rs:36`), added so "older saves predate the field — `false` is the correct reading". That is exactly the masked intra-type change #1714 exists to prevent: a compatibility default silently papers over a shape change instead of the format version gate handling it.
- `saved_type_shape_changes_require_format_major_bump` (serde_default_guard_tests.rs:654) — the serialized-type shape fingerprint moved (`actual=0xdc4ef4bc48ff51d7` vs `BASELINE_SHAPE_FINGERPRINT = 0x0666_f922_7959_3339`) because `ReferenceState` gained `picked_up`, without `byroredux_save::FORMAT_MAJOR` being bumped or the baseline regenerated.

**Impact**

Main's save-format discipline guard is red: any future contributor changing a saved type now hits a pre-existing red guard and cannot tell their change from the P3 one. Substantively, saves written since the P3 commits carry the new field while the format version claims nothing changed — older-format saves rely on the forbidden default instead of the version gate.

**Suggested Fix** (the guards' own prescription — a save-compat decision, which is why it was left to the maintainers rather than fixed in passing with #4458)

1. Bump `byroredux_save::FORMAT_MAJOR` (this is the deliberate step the baseline guard gates on).
2. Remove `#[serde(default)]` from `picked_up`; the format-version gate is the correct mechanism for older saves.
3. Regenerate `BASELINE_MAJOR` / `BASELINE_SHAPE_FINGERPRINT` deliberately per the guard's instructions.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other P3-added saved field carries a compatibility `serde(default)` (the guard scan covers this once green)
- [ ] **TESTS**: Both guards green; a pre-bump save load path is exercised or explicitly version-gated
