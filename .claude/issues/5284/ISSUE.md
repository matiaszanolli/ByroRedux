# #5284: NIFAL-D8-2026-10-05-03: The CDB resolve tail is copied at four sites in `merge.rs`, and the two arm copies skip the `trace_merge_outcome` telemetry the #4289 doc says covers every `PresenceOnly` return

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5284
- **Labels**: low,nifal,tech-debt,bug,game:starfield
- **Source**: `docs/audits/AUDIT_NIFAL_2026-10-05.md` (NIFAL-D8-2026-10-05-03)

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW · **Dimension**: Shader-flags/Effects (merge boundary) · **Tier Violated**: single-boundary (weak, intra-module duplication) · **Game Affected**: Starfield
- **Location**: `byroredux/src/asset_provider/material/merge.rs:482-493` (`.mat`), `:588-601` (unknown kind), `:698-709` (BGSM
  miss), `:1371-1381` (BGEM miss); the doc at `:66-85`.
- **Status**: NEW. `978d25c19` and `18fce7e43` turned the former one-line `apply_cdb_pbr_fallback` tail into this 4×12-line
  block.
- **Description**: Each site repeats `lookup_cdb_material` → `apply_cdb_material` → `Merged`/`PresenceOnly` from `touched`, with
  `apply_cdb_pbr_fallback` on a miss. The BGSM and BGEM arm copies return `Some(outcome)` without `trace_merge_outcome`. Their
  `PresenceOnly` fallback returns were also untraced before this window. The next CDB-arm change (for example D8-01's blend
  forwarding, or recording CDB texture provenance) has to land identically in four places.
- **Suggested Fix**: Extract one private `resolve_through_cdb(material, provider, pool, path, touched, texture_exists) ->
  MergeOutcome` that traces, and call it from all four sites.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
