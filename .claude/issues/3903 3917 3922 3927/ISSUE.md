# #3903 #3917 #3922 #3927 — audit-suite follow-ups

Snapshot as filed; GitHub is authoritative for live state.

## #3903 (LOW, `nifal`, `test-gap`) — texture-roles-deep suite
`record_external_texture_sources` is the fifth hand-written role walk and the
only one with no exhaustiveness guard. Found independently by three audits
(`AUDIT_NIFAL` D8-02, `AUDIT_FO4` D2-03, `AUDIT_STARFIELD` D9-01).

- Location as filed: `byroredux/src/asset_provider/material.rs:1053`.
  **Stale path** — that file became `material/` under #3857; the symbol now
  lives in `byroredux/src/asset_provider/material/merge.rs`.
- Diagnostics-only blast radius (`mat.dump`, `tex.missing` provenance).
- Siblings already guarded by #3349, #3734, #2697.

## #3917 (LOW, `audio`, `import-pipeline`, `game:fnv`) — AUDIT_FNV_2026-09-05 D2-01
`SoundArchiveProvider::extract` is first-listed-wins while #3637 made
mesh/texture/material last-listed-wins, and unlike `ScriptProvider` it carries
no rationale for the difference.

- `byroredux/src/asset_provider/audio.rs`
- Audit assessed impact as **zero on FNV today** ("`default_sounds_bsas` is a
  single archive"). That assessment is stale — see the fix commit.
- Suggested: flip to `.rev()`, or keep first-wins and record why.

## #3922 (HIGH, `renderer`, `shaders`, `game:skyrim`) — AUDIT_SKYRIM_2026-09-05 D2-01
The model-space-normal branch consumes `_msn` maps in the source basis, not the
renderer's.

- `crates/renderer/shaders/triangle.frag`, the `MAT_FLAG_MODEL_SPACE_NORMALS`
  branch inside `normalMapIdx != 0u`.
- 13 656 MSN meshes across the two Skyrim mesh archives; 4 037 in Meshes0 are
  materially affected (3 201 FaceGen heads + 431 armor).
- **Two caveats the issue sets before landing**: (1) re-run the correlation on
  an FO4 `_msn` set, since the branch is shared; (2) render-visible change —
  pair with RenderDoc/screenshot on a FaceGen head, not a unit test alone.
- Secondary: the `!MAT_FLAG_MSN_HAS_AUTHORED_Z` reconstruction arm's sign has
  to be re-derived, not inherited.

## #3927 (HIGH, `renderer`, `shaders`, `game:fo4`) — AUDIT_FO4_2026-09-05b D5-01
`grayscale_to_palette_scale` is a palette-row selector, not a blend weight —
the shader treats it as a `mix()` weight and hardcodes the row at `v = 0.5`.

- `crates/renderer/shaders/triangle.frag`: the lit palette branch
  (`#1353 / FO4-D8-07`); the `MATERIAL_KIND_EFFECT_SHADER` branch above it is
  explicitly **out of scope** (its `v = 0.5` has its own #890 Stage 2c
  rationale for Skyrim FX atlases).
- Made reachable by `79194306` (#3897/#3898); corrects the premise of #2443.
- 30 166 vanilla FO4 LSPs + 477 BGSM materials enter the branch.
