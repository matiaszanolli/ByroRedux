# #6922 — INC-2026-11-14-10: A death sting's silence survives a same-session reload into the same place

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-10). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: MEDIUM
- **Location**: `byroredux/src/music/controller.rs` (`silenced`, reset only in `set`; set in `sting`); `byroredux/src/music/mod.rs` (`music_system` place gating; `end_script_layer`)
- **Changed in**: `controller.rs` (commits `45b3b0a64`, `ad06d54b0`)

## Description
A non-`over_music` sting (FO3/FNV death sting, both Oblivion stings) sets `silenced = true`. It is cleared only by `set(layer != Sting, Some(..))`. `music_system` calls `set(Location, ..)` only when the place, marker, `around` or the OBSE change counter differs from last frame, or when the catalog is rebuilt (`state.place = None` on a new `LoadedCellIndex` address). `end_script_layer` (the cell leave/load hook) only sets `end_script`.

A same-session reload reuses the same `LoadedCellIndex` `Arc` (`SHARED_PARSE`, #6123), so the catalog is not rebuilt; reloading into the same cell or grid completes within one call, so the place never differs and Battle/Location stay suspended as "a sting's silence". `docs/engine/music.md` §9's "after a load, the place changes" is not guaranteed.

## Impact
Die, reload the autosave in the same area: no cell music until the player leaves the area, combat starts, or a script plays music.

## Related
#6901

## Suggested Fix
Clear `silenced` in the `end_script_layer` path (cell leave and load) or on the player-dead falling edge; add a controller test for sting → load → music returns.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other controller state that should reset on load: `battle_hold`, one-shots)
- [ ] **TESTS**: A regression test pins this specific fix
