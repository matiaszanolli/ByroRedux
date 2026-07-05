**Severity**: LOW · **Dimension**: NPC Equip + FaceGen · **Source**: `docs/audits/AUDIT_SKYRIM_2026-07-04.md` (SKY-D3-001)
**Status**: NEW
**Location**: `crates/plugin/src/equip.rs::expand_leveled_form_id`

## Description
The doc comment states the "calculate for each item" flag (LVLF bit 1) is *unimplemented* and that multi-pick LVLIs "land all eligible entries", while describing single-pick as the only real behaviour. The code below branches on `lvli.flags & 0x02` and implements a genuine multi-pick vs single-highest-eligible split; LVLF flags are parsed and populated (`container.rs`, `record.flags = sub.data[0]`), so the branch is live. Doc rot that mis-describes the single-pick semantics (which pick the *highest* eligible, not "all").

## Evidence
```rust
/// … The "calculate for each item" flag (bit 1) is also unimplemented today   // docstring
/// — multi-pick LVLIs land all eligible entries …
let multi_pick = lvli.flags & 0x02 != 0;                                        // contradicts it
if multi_pick { for entry in &eligible { expand_leveled_inner(entry.form_id, ...); } }
else { let pick = eligible.iter().max_by_key(|e| e.level)...; expand_leveled_inner(pick.form_id, ...); }
```
Test `expand_multi_pick_lands_all_eligible` (LVLI flag bit 1 = `0x02`) exercises the multi-pick branch.

## Impact
Documentation only — behaviour is correct. Risk is a future maintainer "adding" a multi-pick that already exists, or mis-reasoning about the single-pick default (highest-eligible, not all) during a leveled-gear audit.

## Suggested Fix
Update the docstring + inline comment to state multi-pick (LVLF bit `0x02`) is implemented and single-pick returns the highest-eligible entry (not "all eligible"). Keep the accurate `chance_none = 0` caveat.

## Related
M41 Phase 2 (#896)

## Completeness Checks
- [ ] **TESTS**: `expand_multi_pick_lands_all_eligible` already pins the behaviour; no new test needed — verify the doc matches it
