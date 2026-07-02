# AUD-2026-07-02-01: SoundCache docstring points to asset_provider.rs (now a directory) instead of asset_provider/texture.rs

**Severity**: LOW
**Dimension**: SoundCache Growth
**Location**: `crates/audio/src/lib.rs:1176-1178`
**Status**: NEW
**Source**: `docs/audits/AUDIT_AUDIO_2026-07-02.md` (AUD-2026-07-02-01)

## Description
The `SoundCache` struct docstring (the "Dormant API (#859)" block) states the footstep path lives at `byroredux/src/asset_provider.rs::try_load_default_footstep`. Post the Session-34 module split, `asset_provider.rs` no longer exists as a single file — it is a directory (`byroredux/src/asset_provider/`), and `try_load_default_footstep` lives in `asset_provider/texture.rs`. The path in the docstring resolves to a file that is not on disk.

## Evidence
```
# crates/audio/src/lib.rs:1176-1178 (current)
/// call sites for `SoundCache`. The footstep dispatch path at
/// `byroredux/src/asset_provider.rs::try_load_default_footstep` writes
/// directly into `FootstepConfig.default_sound: Option<Arc<Sound>>`,

$ ls byroredux/src/asset_provider*
byroredux/src/asset_provider/   (directory)
$ grep -rln 'fn try_load_default_footstep' byroredux/src/
byroredux/src/asset_provider/texture.rs
```

## Impact
Documentation only; no runtime effect. A maintainer following the reference lands on a missing file. This is exactly the stale-path class the `_audit-common.md` path-reference convention targets. Same family as the already-closed #1615 doc-rot fix (which corrected the *function name* in this same block but left the *file path* stale) — this is a residual fix, not a re-occurrence of #1615's bug.

## Related
#1615 (AUD-2026-06-14-04, closed — corrected the stale `resolve_footstep_sound` fn name in the adjacent line); Session-34 module-split layout note.

## Suggested Fix
Change `asset_provider.rs::try_load_default_footstep` to `asset_provider/texture.rs::try_load_default_footstep` in the docstring at `crates/audio/src/lib.rs:1177`.

## Completeness Checks
- [ ] **TESTS**: N/A — single docstring path correction, no behavior change
