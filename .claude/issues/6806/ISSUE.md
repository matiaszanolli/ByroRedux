# ESM-D7-06: `asset_provider/audio.rs` comment says no REGN-driven runtime exists for Skyrim `MUSC`/`MUST`; the code above it is that runtime

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6806
Labels: low, esm-plugin, documentation, doc-rot, audio
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW
- **Location**: `byroredux/src/asset_provider/audio.rs:287-297` (comment) vs `:62-65`, `:283` (`resolve_music_track_path` -> `first_playable_track`), and the log text at `:306-311`
- **Status**: NEW (stale since `03f9d2158`, 2026-11-04, which added the MUSC/MUST resolution)
- **Description**: the comment claims "no REGN-driven runtime exists for Skyrim `MUSC`/`MUST` or FNV `MSET`" and the once-only log says "neither has a playback runtime yet (#3816)". Skyrim `RDMO` -> `MUSC.TNAM` -> `MUST.ANAM` now resolves and plays (first playable single track, cycling/loop regions explicitly unmodelled). Only FNV `MSET` and Oblivion `RDMD` are still unresolved.
- **Suggested fix**: reword the comment and the log to name only `MSET` (FNV) and `RDMD` (Oblivion) as unsupported.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
