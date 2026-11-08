# ESM-D3-03: `parse_musc` is a public `&None` wrapper over a record that now carries FormIDs, with a stale exemption

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6788
Labels: low, esm-plugin, bug, game:skyrim
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (no live caller)
- **Dimension**: 3 (guard blind spot, shape 1 flavour)
- **Record/Sub-record**: MUSC / TNAM (Skyrim)
- **Location**: `crates/plugin/src/esm/records/misc/music.rs:47-52` (`parse_musc` -> `parse_musc_remapped(.., &None)`),
  `records/misc.rs:75` and `records/mod.rs:89` (re-exports), `records/tests.rs:2984-2985`
  (`EXEMPT_NO_U32_READS` entry `("misc/music.rs", "parse_musc")` with the comment "MUSC is EDID + a track path + one f32 level; no FormIDs")
- **Status**: NEW (introduced by `03f9d2158`)
- **Description**: Since Skyrim `TNAM` (array of MUST FormIDs) joined the record, `parse_musc` is exactly the
  thing the guards exist to stop: a remap-free entry point to a FormID-bearing decoder. The guards miss it twice:
  `parse_musc` has no `u32` read itself (it delegates), so `EXEMPT_NO_U32_READS` re-verifies mechanically; and
  `no_parser_with_a_remap_in_scope_passes_an_identity_remap` only fires when a remap is in scope.
- **Evidence**: `grep -rnE 'parse_musc\b'` shows no caller outside tests and the re-exports. The dispatcher
  (`dispatch_misc_stub.rs:99`) calls `parse_musc_remapped(.., &remap)`. The 5 `parse_musc(...)` uses in `music.rs` are
  unit tests.
- **Impact**: None today. A future caller gets Skyrim track ids in plugin-local space, which silently misses every
  `music_tracks` lookup on a plugin with masters.
- **Suggested fix**: Drop the wrapper and the exemption (tests call `parse_musc_remapped(.., &None)`), or `#[cfg(test)]`
  the wrapper.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
