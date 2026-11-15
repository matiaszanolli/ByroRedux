# #6955: TD2-2026-11-15-02: Five LOD ring reconcilers each carry the same "admission gate, budget, attempt, classify, complete" loop; #6092 had to edit four of them identically and the fifth was copied from them

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6955
- **Labels**: low,terrain-exterior,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: medium · **Labels**: `tech-debt` · **Existing**: none (#5820 consolidated only the unload half, `retain_resident`)
- **Age**: #6092 635330d08 (2026-10-23) touched four copies; `tree_lod_legacy.rs` 510e7dc4d / 8e7624256 (2026-11-08) is the fifth
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:731-844` (`stream_lod_blocks`), `object_lod.rs:327-377` (`stream_object_lod_blocks`), `placement_lod.rs:406-447` (`stream_placement_lod_blocks`), `tree_lod.rs:594-631` (`stream_tree_lod_blocks`), `tree_lod_legacy.rs:463-495` (`stream_legacy_tree_lod_blocks`)
- **Description**: After `retain_resident` unloads, every ring runs the same shell: filter `candidates` not yet resident; `if !ctx.mesh_registry.<tier>_geometry_admission_open() { budget.note_held_back(remaining); starved = true; break }`; `if !budget.try_take() { break }`; call a spawn function; on `None`, `match classify_lod_attempt(false, <admission again>) { Miss => insert sentinel, HeldBack => { budget.note_held_back(remaining + 1); starved = true; break }, Keep => unreachable!() }`; then `complete = starved || <all attempted>`. The doc on `retain_resident` (`lod_support.rs:485`) says it exists so "one body serves all four rings" and the leak-fix class lands in one place; the load half has the opposite property. Two spellings of the bookkeeping already exist: terrain/object/placement count `attempted` and test `attempted == candidate_count` (remaining = `candidate_count - attempted (+1)`), while tree and legacy-tree test `candidates.iter().all(|k| ring.blocks.contains_key(k))` and use the loop index. They are equivalent today. The `+1` in the held-back arm is exactly the kind of off-by-one that gets fixed in one copy.
- **Suggested Fix**: add `lod_support::load_candidates<K: Copy, B>(candidates: &[K], budget: &mut LodWorkBudget, admission_open: impl Fn() -> bool, mut spawn: impl FnMut(K) -> Option<B>, mut sentinel: impl FnMut(K) -> B, mut insert: impl FnMut(K, B)) -> LoadOutcome { attempted, starved }` beside `retain_resident` and `classify_lod_attempt`, and migrate the five callers (each keeps its own spawn call and per-ring log line). `streaming_tests.rs:977-980` text-pins the admission call in four of the five files (terrain, object, placement, tree); re-point it at the helper's call sites in the same commit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
