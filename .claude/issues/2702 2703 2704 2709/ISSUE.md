# Issues 2702, 2703, 2704, 2709

All filed from `docs/audits/AUDIT_FO4_2026-08-12.md` (2702-2704) and
`docs/audits/AUDIT_STARFIELD_2026-08-12.md` (2709). Domain: binary
(byroredux asset_provider / material_translate), one renderer doc fix.

## #2702 — FO4-D2-03: BGSM Phase-1 flag-forwarding tests are mirror tests

`byroredux/src/asset_provider/tests.rs:784-816`, `:820-856`, `:862-894` —
`bgsm_merge_forwards_phase1_shader_flags`, `bgsm_merge_does_not_set_phase1_flags_from_false`,
`bgsm_merge_phase1_flags_honor_child_first_chain` each declare a local
`let mut is_pbr = false;` and hand-copy the production merge logic instead of
calling `merge_external_material`. They assert their own copy, not production
behaviour — same class as the `is_pbr` contract flip in FO4-D2-01 that landed
green while a test comment stated the opposite.

Fix: rewrite against the real `merge_external_material`, mirroring the
`insert_bgem_for_test` fixture shape for BGSM.

Severity: low. Labels: bug, tech-debt.

## #2703 — FO4-D7-01: GpuMaterial::ior doc falsely claims FO4 BGSM v9+ authors IOR

`crates/renderer/src/vulkan/material.rs:232-242` doc says "FO4 BGSM v9+ and
Starfield .mat materials author this explicitly." `crates/bgsm/src/bgsm.rs`
decodes no IOR/refraction field at any version — v>=9's addition is
`custom_porosity`/`porosity_value`. `merge_external_material` never assigns
`ior` for FO4; every FO4 material takes the generic dielectric default /
glass promotion via `material_optical_scalar`. Claim is true only for
Starfield `.mat`.

Fix: drop the FO4 clause or replace with "FO4 BGSM authors no IOR; the FO4
path always takes the dielectric default."

Severity: low. Labels: documentation, renderer.

## #2704 — FO4-D7-02: Eleven BGSM scalars decoded with no sink and no deferral comment

`crates/bgsm/src/bgsm.rs:68-101` parses but drops (no `byroredux/src/asset_provider/material.rs`
reader) — the entire wetness-control suite (`wetness_control_spec_scale`,
`_spec_power_scale`, `_spec_min_var`, `_env_map_scale`, `_fresnel_power`,
`_metalness`), plus `custom_porosity`, `porosity_value`,
`adaptive_emissive_exposure_offset`, `aniso_lighting`, `external_emittance`.
No comment marks the omission as deliberate, unlike `distance_field_alpha_texture`
and the BGEM glass-overlay suite. Same class as OPEN #2607/#2608/#2627/#2642
(distinct fields). Relevant to ROADMAP M61 wet-surface feature.

Fix: add one grouped `// Deferred: no consumer` comment in
`merge_external_material` naming these fields.

Severity: low. Labels: bug, import-pipeline, tech-debt.

## #2709 — SF-D9-03: merge_external_material's bool return can't distinguish empty merge

`byroredux/src/asset_provider/material.rs:667-739`. Doc says `touched` "flips
to true on any merged field", but the `.mat` arm returns `true` after setting
only `is_pbr` — no textures/scalars/alpha forwarded. All 5 production call
sites (`cell_loader/references/import.rs:113`, `cell_loader/partial.rs:115`,
`scene/nif_loader.rs:273`, `cell_loader/precombined.rs:275`) discard the
return value; only test code asserts on it. No telemetry distinguishes
"resolved" from "resolved to nothing" — relevant since 97.9% of Starfield
content is in that state (total texture blackout, Dim 8, produces no log
line).

Suggested fix: mark `#[must_use]` + per-cell counter, OR return a small enum
(`Unresolved` / `Merged { fields: usize }` / `PresenceOnly`).

Severity: low. Labels: bug, tech-debt.
