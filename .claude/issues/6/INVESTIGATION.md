# Investigation: Issue #6

## What Was Done

### 1. NiDefaultAVObjectPalette parser (NEW)
`crates/nif/src/blocks/palette.rs` — parses the object palette block:
- `scene_ref: BlockRef` — scene root
- `objs: Vec<AVObject>` — name → block ref mapping
- `find_by_name()` utility for name-based lookup
- Registered in block dispatch at mod.rs

### 2. import_kf() extended for NiControllerManager
`crates/nif/src/anim.rs` — now discovers sequences two ways:
- **Path 1**: NiControllerManager → follow sequence_refs (embedded .nif anims)
- **Path 2**: Top-level NiControllerSequence (standalone .kf files)
- Deduplication via seen_indices HashSet prevents double-import
- Debug logging for manager-sourced sequences

### 3. Acceptance Criteria
- [x] .nif files with NiControllerManager auto-discover sequences
- [x] Object palette parser exists for future node binding
- [ ] Cumulative mode flag is parsed but not yet wired to AnimationPlayer
  (would need AnimationStack changes — separate follow-up)

## Scope
3 files modified: palette.rs (new), mod.rs (dispatch entry), anim.rs (import logic)
