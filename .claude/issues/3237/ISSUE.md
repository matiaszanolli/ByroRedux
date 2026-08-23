# 3237: SAFE-D2: ESM/ESP GRUP-tree recursion has no depth bound — crafted plugin stack-overflows the process

**Severity**: HIGH · **Dimension**: Safety Dimension 2 (Memory Corruption/UB — stack-overflow recursion risk) · **Report**: `docs/audits/AUDIT_SAFETY_2026-08-23.md` (SAFE-D2-2026-08-23-01)

## Description

Every GRUP-tree walker in the ESM/ESP parser recurses into nested groups unconditionally — there is no depth counter, no `MAX_..._DEPTH` constant, and no cap analogous to the NIF importer's `MAX_NIF_NODE_DEPTH` (128, `crates/nif/src/import/walk/mod.rs`) or the collision-shape resolver's `MAX_COLLISION_SHAPE_DEPTH` (64, `crates/nif/src/import/collision/shape.rs`, #1385/MEM-06) — both confirmed intact and are the reference model for a fix.

A `GRUP` header is only 20-24 bytes (`EsmReader::read_group_header`, `crates/plugin/src/esm/reader.rs:563-583`), and `group_content_end` (`reader.rs:717-719`) trusts the file's own `total_size` field with only a `saturating_sub` floor. Nothing stops a crafted plugin from nesting one minimal GRUP directly inside another, recursively, for as many levels as the file's byte budget allows.

Affected walkers: `extract_records` / `extract_records_with_modl` / `extract_dial_with_info` / `extract_quest_dialogue_scene_tree_inner` (`crates/plugin/src/esm/records/grup_walker.rs`); `parse_wrld_children` (`crates/plugin/src/esm/cell/wrld.rs`, group types 4/5/6); `parse_cell_group` (`crates/plugin/src/esm/cell/walkers.rs`, group types 2/3).

`extract_records`/`extract_records_with_modl` are reached from every major top-level GRUP dispatcher (`dispatch_global.rs`, `dispatch_world_placement.rs`, `dispatch_misc_stub.rs`, `dispatch_misc_gameplay_a.rs`, `dispatch_misc_gameplay_b.rs`, `dispatch_container.rs`, `dispatch_actor.rs`, `dispatch_items.rs`) — this is not a narrow code path, it is hit on every ESM/ESP a user loads, including third-party mod content.

## Evidence

```rust
// crates/plugin/src/esm/records/grup_walker.rs:65-72
pub(super) fn extract_records(
    reader: &mut EsmReader, end: usize, expected_type: &[u8; 4],
    f: &mut dyn FnMut(u32, &[SubRecord]),
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);
            extract_records(reader, sub_end, expected_type, f)?;   // no depth arg, no cap
            continue;
        }
        ...
```
Compare to the NIF equivalent (`crates/nif/src/import/walk/mod.rs:220-236`), which threads a `depth: u32` and bails past `MAX_NIF_NODE_DEPTH` — no such parameter exists anywhere in `grup_walker.rs`, `wrld.rs`, or `cell/walkers.rs`.

## Impact

A malicious or merely corrupt `.esm`/`.esp` plugin (a few hundred KB to a few MB, at 24-28 bytes per nesting level) can drive the recursion tens of thousands of levels deep, overflowing the native stack and aborting the whole engine process — not a graceful `Result`-typed parse failure, but an uncatchable crash. Blast radius is every game variant, since the affected walkers are reached from all eight top-level record dispatchers, and this is exactly the untrusted-input class the project already hardens the NIF/BSA readers against (`MAX_SINGLE_ALLOC_BYTES`, `check_alloc`, `MAX_NIF_NODE_DEPTH`, `MAX_COLLISION_SHAPE_DEPTH`).

## Related

Previously identified in `docs/audits/AUDIT_ESM_2026-08-13.md` as **ESM-D1-04** (filed there as MEDIUM under that report's own per-record-domain scale) but never converted into a tracked GitHub issue — this is the first tracking issue for it. Rated HIGH here per `_audit-severity.md`'s shared decision tree: a hard, uncatchable process abort from untrusted input with no recovery path is closer to the "parse failure prevents loading game content" HIGH-minimum row, and worse in effect since it takes down the whole engine.

Regression-guard precedent that already solves this exact class of problem: `crates/nif/src/import/walk/mod.rs` (`MAX_NIF_NODE_DEPTH`, #1269) and `crates/nif/src/import/collision/shape.rs` (`MAX_COLLISION_SHAPE_DEPTH`, #1385/MEM-06).

## Suggested Fix

Thread a `depth: u32` parameter through `extract_records`, `extract_records_with_modl`, `extract_dial_with_info`, `extract_quest_dialogue_scene_tree_inner`, `parse_wrld_children`, and `parse_cell_group`, and bail out (skip the group, log a warning) past a shared `MAX_GRUP_NESTING_DEPTH` constant — mirroring `MAX_NIF_NODE_DEPTH`'s pattern exactly. A generous ceiling (e.g. 32-64; vanilla content nests at most 3-4 GRUP tiers deep) costs nothing on legitimate files and closes the crash.

## Completeness Checks
- [ ] **SIBLING**: All six affected walkers get the same depth-cap treatment, not just the most-reached one
- [ ] **TESTS**: A regression test with a synthetic deeply-nested GRUP file asserting graceful `Result::Err`/skip instead of a crash
