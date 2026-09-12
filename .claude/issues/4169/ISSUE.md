# ESM-2026-09-11-D5-01: PlacedRef.group_type is still 6 for every real placement — regression of #3728

URL: https://github.com/matiaszanolli/ByroRedux/issues/4169
Labels: bug, medium, esm-plugin, doc-rot

---

**Severity**: MEDIUM
**Dimension**: CELL / WRLD Walkers & Placement Data
**Record / Sub-record**: CELL/WRLD child GRUPs 6/8/9/10 — `REFR`/`ACHR`/`ACRE`/`PGRE`/`PHZD`/`PMIS`
**Location**: `crates/plugin/src/esm/cell/walkers.rs:161-177,702-738`; `crates/plugin/src/esm/cell/wrld.rs:276-329`; `crates/plugin/src/esm/cell/mod.rs:391-406,320-322`; `crates/plugin/src/esm/cell/tests/cell.rs:198-199`
**Status**: Regression of #3728 (CLOSED) — re-verified still present at HEAD by direct source re-read. Not superseded by `6c3584ec`/#4077, which fixed a related but distinct false-topology premise in the immediately neighbouring code, but left this bug and its stale legend comments untouched.

**Description**: `parse_refr_group_inner` receives `group_type` once, at the point the outer 6/8/9 container group is entered, and threads that same value unchanged through its own nested-group recursion without ever re-reading `sub.group_type`. Real Bethesda content always nests 8 (Persistent) / 9 (Temporary) / 10 (Visible Distant) *inside* a type-6 "Cell Children" container — never as its direct sibling — so every placement in every shipped master is stamped `group_type = 6`, discarding exactly the persistent/temporary/visible-distant distinction #3728 was filed to add.

**Evidence** (`walkers.rs:702-728`):
```rust
fn parse_refr_group_inner(
    ...
    group_type: u8,   // threaded UNCHANGED through recursion
    depth: u32,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            ...
            parse_refr_group_inner(
                reader, sub_end, refs, landscape, navmeshes, pathgrids, deleted,
                group_type,  // <-- never re-derived from `sub.group_type`
                depth + 1,
            )?;
            continue;
        }
```
The comment immediately above this parameter still documents the bug as intended behaviour ("Fixed for the entire recursion … threaded unchanged (unlike `depth`, which increments)"). The regression fixture (`tests/cell.rs:198-199`) still builds two direct CELL children (type 6 and type 8 as siblings) — a shape that occurs zero times in any shipped master, which is why the test stays green while the bug stays live. No live consumer exists yet (`grep -rn '\.group_type'` outside walker/reader/tests finds only two `0xFF`-sentinel test fixtures).

**Impact**: A field that looks authoritative, is guarded by a green but unrepresentative test, and is uniformly wrong. It will silently misinform the first streaming-residency or save-restore consumer that reads it.

**Related**: #3728 (closed, inert fix), #4077/`6c3584ec` (fixed the neighbouring false-topology premise but not this bug). Includes ESM-2026-09-11-D5-02's stale-comment cleanup (four sites: `walkers.rs:161`, `wrld.rs:311`, `mod.rs:391-393,320-322`) as part of the same fix.

**Suggested Fix**: Re-derive `group_type` at each nested group inside `parse_refr_group_inner` — when `sub.group_type` is 8/9/10, pass that value into the recursive call instead of the inherited one. Correct the stale legend comments at all four sites and rebuild the test fixture as CELL → GRUP 6 → {GRUP 8, GRUP 9, GRUP 10}, the only nesting shape the real corpus contains.

## Completeness Checks
- [ ] **TESTS**: Rebuild `tests/cell.rs:198-199`'s fixture to the real GRUP-6-wraps-{8,9,10} nesting shape and pin the corrected `group_type` per placement

