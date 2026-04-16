# D4-01/D4-06: NiAVObject flag mask 0x21 mislabels APP_CULLED; shape vs node disagree

## Finding: D4-01 / D4-06 (LOW)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md` / Dimension 4
**Games Affected**: All games (Oblivion → Starfield)
**Location**:
- `crates/nif/src/import/walk.rs:126, 158, 253, 290, 373` (NiNode, uses `& 0x21`)
- `crates/nif/src/import/walk.rs:205, 220, 332, 346` (shape paths, use `& 0x01`)

## Description

Walker skips NiNode blocks when `node.av.flags & 0x21 != 0`, intended to filter "hidden OR editor-marker" (commit 5e6c27b). Per Gamebryo 2.3 source (`NiAVObject.h:226-244`):

```cpp
APP_CULLED_MASK     = 0x0001,   // the actual hidden bit
DISPLAY_OBJECT_MASK = 0x0020,   // occlusion display helper — SHOULD render
```

The commit labeled `0x20` as APP_CULLED (wrong). Net effect: nodes tagged with the occlusion-display bit are silently skipped. Editor markers are actually filtered elsewhere (BSXFlags bit 5 + name prefix in `is_editor_marker`), so the `0x20` gate is redundant on vanilla content and over-aggressive on mods that use the display-object flag.

Additionally, `NiTriShape` / `BsTriShape` use `& 0x01` (correct). The two code paths disagree on what "hidden" means.

## Impact

In practice most Bethesda vanilla NIFs don't set `0x20` so no visible bug today. Modded content or engines following the Gamebryo convention would have nodes silently dropped.

## Suggested Fix

Use `& 0x01` (APP_CULLED only) on all 5 NiNode sites. If editor-marker filtering via flags is desired, move it to BSXFlags rather than NiAVObject flags. Unify shape and node behavior.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
