# EXT-D1-2026-10-31-02: #6464 spliced a new test between `the_cache_and_the_processing_step_share_one_gate` and its doc comment

Labels: low,terrain-exterior,documentation,doc-rot

Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-31.md` (finding `EXT-D1-2026-10-31-02`).

- **Severity**: LOW (doc-rot)
- **Dimension**: 1 EXAL boundary
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: `byroredux/src/cell_loader/load_order.rs:1957-1969`
- **Status**: NEW. This is the same class as closed (issue 6373) / #6454 and open-then-fixed (issue 6410), and `977936719` is its cause.
- **Description**: The doc "The step's gate and the cache's `exterior_ready` are one predicate: every game the step processes is re-parsed for an exterior session, and no other is." used to head `the_cache_and_the_processing_step_share_one_gate`. The commit inserted `use_land_data_children_inherit_on_oblivion_fo3_fnv_and_fo4_only` directly under that doc, so the doc now describes the new gate-membership test. The sharing test (:1969) has no doc.
- **Evidence**:
  ```rust
  /// The step's gate and the cache's `exterior_ready` are one predicate: ...
  #[test]
  fn use_land_data_children_inherit_on_oblivion_fo3_fnv_and_fo4_only() {   // :1960
  ...
  #[test]
  fn the_cache_and_the_processing_step_share_one_gate() {                 // :1970, no doc
  ```
- **Impact**: Misleading test documentation on the exact predicate that #6407 and #6464 turned on. `source_scan::spliced_doc_blocks` trips only when the doc leads with a sibling's name, so it cannot see this.
- **Suggested Fix**: Give the new test its own doc (the measured FO4 children and the Skyrim "not measured" note already in its body) and put the "one predicate" sentence back above `the_cache_and_the_processing_step_share_one_gate`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
