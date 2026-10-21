**HEAD**: c4df4c91e · **Baseline**: AUDIT_INCREMENTAL_2026-09-09.md · **Audited**: exterior Dims 1-4 (terrain/height blend, ground-cover boundary, sky dome), renderer (GPU-struct lockstep, shader/SPIR-V parity, terrain-tile SSBO), ecs/tooling (console commands, settings registry), tech-debt (audit infrastructure + doc rot), performance/concurrency (hunk-level) · **Unchanged since baseline (skimmed)**: none — every routed owner had a hunk; nifal routed by `gpu_types.rs` but no `Material` path changed (no NIFAL check applicable)

# Incremental Audit — 2026-10-20 (`--commits 5`, HEAD~5..HEAD)

Run as the `/audit-suite --preset quick` leg. Read-only; no engine launched; no issues created.

## 1. Change summary

Scope: `git diff HEAD~5..HEAD` (2975d2906..c4df4c91e), 59 files, +1148 / -438.

| Commit | Theme |
|---|---|
| 2975d2906 | `terrain.height all` area census; BTXT-only (tile-less) cells now queue their base height map at spawn (part of #6195, still OPEN) |
| 1a2330361 | FO4 `commonwealth` joins the arid ground-cover climate keywords (art direction); unit test + `--ignored` FO4 census test |
| ea45af067 | #6196 height-based terrain layer blending: `byroTerrainHeightBlend` in `bindings.glsl`, used by `triangle.frag` + `groundcover_blade.frag`; band rides `GpuTerrainTile`'s former tail pad (`pad_to_stride` → `height_blend_band`, still @204, stride 208); new slider setting `render.terrain_height_blend_band` (default 0.2) broadcast every frame by `VulkanContext::set_terrain_height_blend_band`; `terrain.blend` console command. Commit has no message body (no frame-time delta, no test). |
| ae557b935 | SKYAL: WTHR sky-dome zenith/horizon/lower colours sRGB-decoded once in `render/sky.rs::outdoor_sky_params` (`dome_colour_radiance` → `fog::fog_colour_radiance`) |
| c4df4c91e | Audit-skill / owner-map / audit-sync tooling re-sync (35 `.claude/` files) |

## 2. Routing map

`_audit-route.sh` routed every path; **no `(unrouted)` paths**.

| File(s) | Owner dims audited | Risk |
|---|---|---|
| `crates/renderer/shaders/include/bindings.glsl`, `triangle.frag`, `groundcover_blade.frag`, all touched `.spv` | renderer (GPU struct, shader parity), exterior Dim 2/3, performance | HIGH |
| `crates/renderer/src/vulkan/scene_buffer/{gpu_types,gpu_instance_layout_tests}.rs` | renderer (lockstep), nifal (n/a) | HIGH |
| `crates/renderer/src/vulkan/context/resources.rs` | renderer, concurrency, performance, safety (no `unsafe` added) | HIGH |
| `byroredux/src/app_frame.rs`, `crates/core/src/settings/builtin.rs` | renderer, ecs, concurrency, performance | HIGH |
| `byroredux/src/render/sky.rs` | exterior Dim 1/4 (colour-space rule), renderer | MEDIUM |
| `byroredux/src/cell_loader/terrain.rs`, `byroredux/src/terrain_height.rs` | exterior Dim 2, concurrency Dim 7, performance Dim 7 | MEDIUM |
| `byroredux/src/groundcover_translate{,_tests}.rs` | exterior Dim 3 boundary | MEDIUM |
| `byroredux/src/commands/{mod,terrain}.rs`, `byroredux/src/main.rs`, `crates/debug-ui/src/lib.rs` | ecs, tooling | MEDIUM/LOW |
| `docs/engine/debug-cli.md` | tech-debt (doc rot) | LOW |
| `.claude/**` (35 files) | tech-debt (audit infrastructure) | LOW |

## Verified clean (checks that passed)

- **GPU-struct lockstep**: `GpuTerrainTile` field renamed in place at offset 204 in both Rust and `bindings.glsl`; no other mirror of the struct exists (`git grep pad_to_stride|padToStride` → 0 hits). `cargo test -p byroredux-renderer --lib` → 1443 passed (incl. `gpu_terrain_tile_is_208_bytes`, `gpu_terrain_tile_field_offsets_match_shader_contract`, `gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep`).
- **SPIR-V staleness**: every shader that includes `bindings.glsl` (16 sources + the `-DBYRO_OPAQUE_EARLY_TESTS=1` triangle variant) was recompiled to scratch and is byte-identical to the shipped `.spv`; only the four binaries that declare `GpuTerrainTile` (`triangle`, `triangle_early`, `groundcover_blade`, `water` — the latter only its `OpName` string) needed rebuilding, and all four were. `scripts/check-shader-artifacts.sh` → 38 shaders + early variant match (glslang 11:16.4.0).
- **New setting wiring**: registered in `register_builtin_settings` (slider 0..1, step 0.01), re-exported through `byroredux_debug_ui`, read per frame in `app_frame.rs:551`; `validate_value` rejects NaN/inf/out-of-range, so `terrain.blend nan` cannot reach the per-frame `!=` compare (which would otherwise dirty the SSBO every frame).
- **Colour-space rule (sky.rs)**: decode is applied once, CPU-side, to zenith/horizon/lower only; sun colour and cloud tints stay raw; the interior portal path re-enters `outdoor_sky_params`, so `exterior_zenith_color` and `portal_outdoor_sky` carry the decoded value; no shader-side second decode exists (`sky_cube.comp`, `composite.frag`, `sky_irradiance.comp`, `include/*.glsl` grep). `sky.dump` reads `build_sky_params`, so it reports what the GPU receives.
- **Debug commands documented**: `terrain.height [all]` and `terrain.blend [band]` are in `docs/engine/debug-cli.md`; the documented count (134) equals the registry (134 `registry.register(` calls) and the count test passes.
- **Lock order**: `terrain.height all` holds a `TerrainLayerHeights` query then reads `TerrainHeightService` (both read); `apply_ready_heights` drops its service write before querying; spawn-side `service(world)` takes `&mut World`. No ABBA pair introduced.
- **Bin tests**: `cargo test -p byroredux --bin byroredux` → 3078 passed, 0 failed (incl. updated sky.rs and groundcover_translate tests). `cargo test -p byroredux-core --features inspect --lib settings` → pass.
- **`.claude/` commit**: `_audit-validate.sh` → "OK: all path references valid"; zero advisory symbols in any skill file (all advisories are in `docs/engine/` and the pre-existing `CRHoldExpansion` in CLAUDE.md). Every new `_audit-owners.md` row points at an existing path and first-match-routes as intended (spot-routed 25 paths).

## 3. Findings

### INC-2026-10-20-01: `byroTerrainHeightBlend` is discontinuous where a layer's painted share reaches 0 — hard, triangle-edge-aligned seams of up to 50 % layer coverage at every painted-layer rim
- **Severity**: HIGH (risk floor of the `bindings.glsl` / `triangle.frag` rows; on impact alone this is a visual artifact with a workaround, `terrain.blend 0`)
- **Dimension**: exterior Dim 2 (height blend) / renderer
- **Location**: `crates/renderer/shaders/include/bindings.glsl:595` (`hasHeight[i] = idx != 0u && share[i] > 0.0`), `:603` (`count < 2u` early return), `:609-619` (scoring)
- **Changed in**: `crates/renderer/shaders/include/bindings.glsl` (commit ea45af067)
- **Status**: NEW (implementation defect inside open epic #6196)
- **Description**: A height-mapped lane scores `h_i + share_i / S`. As its painted share tends to 0 its score tends to `h_i`, not to "nothing", so a lane with a high height texel keeps a large blended share for arbitrarily small `share_i`. At exactly `share_i == 0` the lane is excluded (`hasHeight` false), and with fewer than two lanes in play the function returns the unblended weights. The blended share therefore jumps from up to 50 % to 0 across the zero-weight boundary. Splat weights are per-vertex UNORM8 interpolated, so that boundary is the triangle edge between a triangle with one non-zero vertex and an all-zero triangle: the seam is polygonal and follows the LAND grid. The same jump occurs on the base lane where a painted layer reaches full opacity (base share → 0).
- **Evidence**: CPU mirror of the function (`/tmp/audit/quick-2026-10-20/incremental/hb.py`), one painted lane over the base: band 0.2, `h_layer=1.0, h_base=0.0` → share' **0.500 at w=1e-4, 0.000 at w=0**; `h_layer=0.95, h_base=0.1` → 0.201 → 0.000; band 1.0, `h_layer=0.7, h_base=0.3` → 0.286 → 0.000. In Mishkinis' two-layer original the weights are complementary and no lane is ever excluded, so the formula never had this edge; the N-lane generalisation added the `share > 0` gate without making the score vanish with the share.
- **Impact**: Every terrain cell with at least two height-mapped lanes (the BTXT base always has one once generated) shows speckled bleed of high-height texels that cuts off along triangle edges at each painted layer's outer rim and at fully opaque cores. The seam gets wider as the band grows (at band 1.0 it appears wherever `h_layer > h_base`). It also feeds the specular/normal loops, the blade ground colour, and the Tier-3/receiver density (see -02). It violates #6196's requirement that the blend respect the painted weights.
- **Related**: #6196 (open; acceptance "Review by Matías" pending), #6197 (POM driven by the blended height inherits the seam).
- **Suggested Fix**: Make each lane's blended share go to 0 continuously with its painted share. For example, multiply each lane's `max(score − floor, 0)` by its `share_i` (or by `smoothstep(0, ε, share_i)`) before normalising, or score with `h_i · share_i`-weighted terms. Then remove the hard `share > 0` and `count < 2` cut-offs, or make them no-ops at the limit. Add a CPU-mirror test asserting continuity as `share_i → 0`.

### INC-2026-10-20-02: Height-blended splat now feeds the terrain receiver's `d_ground` and Tier-3 detail density, while the blade scatter still evaluates the unblended field — breaks the documented "same density field" invariant
- **Severity**: MEDIUM (the split is partly deliberate: `triangle.frag:337-342` names Tier-3 as an intended consumer of the blended lanes. What is wrong is the broken invariant and the comments/doc that still state it. Visual-only impact.)
- **Dimension**: exterior Dim 3 (ground-cover pipeline) — silent divergence
- **Location**: `crates/renderer/shaders/triangle.frag:337-349` (in-place rewrite of `terrainSplat`), `:431-432` and `:2899-2900` (Tier-3 detail + canopy receiver read the rewritten lanes); `crates/renderer/shaders/groundcover_scatter.comp:219-224` (scatter density from `byroSampleTerrain`, unblended)
- **Changed in**: `crates/renderer/shaders/triangle.frag` (commit ea45af067)
- **Status**: NEW
- **Description**: `triangle.frag:2875-2880` states that the receiver evaluates "the *same* `byroGcDensityGround` the scatter runs ... so the shadow's density and the density of the sward casting it cannot disagree", and that the interpolated splat weights "are the same numbers `byroSampleTerrain` would return". `exal-groundcover.md:1339` says the same. After ea45af067 the fragment-side lanes are height-reweighted before both density evaluations. `groundcover_scatter.comp`, `groundcover_blade.vert` and `groundcover_models.comp` still sample raw splat weights through `byroSampleTerrain`.
- **Evidence**: `triangle.frag:345-349` calls `byroTerrainHeightBlend(terrainSplat[0], terrainSplat[1], ...)`. `:431` `gcDetailSample.splat0 = terrainSplat[0];`. `:2899` `gcSample.splat0 = terrainSplat[0];`. `groundcover_scatter.comp:219` `TerrainSample s = byroSampleTerrain(...)` with no height blend.
- **Impact**: Where the blend reshuffles shares between a grassy lane and a bare lane (rock poking through dirt, which is the feature's purpose), contact darkening and Tier-3 species detail follow the blended surface while blades are still placed by the painted weights. The result is shadowed patches with no blades, or blades over undarkened ground. The comments and doc above are now false. With -01 this mismatch also inherits the rim seam.
- **Suggested Fix**: Decide which field is canonical. Either feed the receiver/Tier-3 the pre-blend lanes (keep a copy before line 345), or apply the same `byroTerrainHeightBlend` inside `byroSampleTerrain` consumers (scatter/models) so all density evaluations agree. Then correct `triangle.frag:2875-2880` and `exal-groundcover.md` §"How the terrain receiver gets `d_ground`".

### INC-2026-10-20-03: `terrain.blend` claims a session-only change "without saving it", but the next unrelated settings save persists it
- **Severity**: LOW
- **Dimension**: tooling / ecs (settings contract)
- **Location**: `byroredux/src/commands/terrain.rs:89-92` (doc), `:113-117` (`world_resource_set::<SettingsRegistry>` → `registry.set`); `byroredux/src/main.rs:1754-1757`; `crates/settings-io/src/lib.rs:268-273`
- **Changed in**: `byroredux/src/commands/terrain.rs` (commit ea45af067)
- **Status**: NEW (related class: #4975, closed: registry/live-state divergence persisting an unintended value)
- **Description**: The command writes the live `SettingsRegistry` directly. `settings_io::save` writes every entry whose value differs from its last-synced value (not only the entry that changed), and any later settings-menu change triggers that save. The console-set band therefore lands in `settings.toml` at the next unrelated settings save, contrary to the doc comment and the `debug-cli.md:478` "for this session" wording.
- **Evidence**: `save_to_path`: `for entry in registry.entries() { if pinned.contains(&entry.id) || synced.get(&entry.id) == Some(&entry.value) { continue; } settings.insert(...) }`.
- **Impact**: An A/B experiment value (e.g. `terrain.blend 0`) can silently become the user's persisted default.
- **Suggested Fix**: Either drop the "without saving" claim (and say it persists with the next settings save), or make the console path session-only, e.g. a separate runtime override resource read by `app_frame.rs`, or pin the id in `SettingsPersistence` for the session.

### INC-2026-10-20-04: `setting_number` is a copy of `setting_bool` instead of one generic accessor
- **Severity**: LOW
- **Dimension**: tech-debt (duplication)
- **Location**: `byroredux/src/main.rs:1598-1607` vs `:1609-1618`
- **Changed in**: `byroredux/src/main.rs` (commit ea45af067)
- **Status**: NEW
- **Description**: The new helper repeats the `try_resource::<SettingsRegistry>() → get(id) → clone → match variant → unwrap_or(fallback)` chain line for line with only the variant changed. Repo/global guidance prefers extending the existing helper over duplicating it. A third typed reader (Choice) would make a third copy.
- **Suggested Fix**: One `fn setting_value(world, id) -> Option<SettingValue>` (or a generic over an extractor closure) with `setting_bool` / `setting_number` as thin wrappers.

### INC-2026-10-20-05: Doc rot — `GpuTerrainTile`'s tail is still described as stride padding, and the height blend is undocumented in the engine docs
- **Severity**: LOW
- **Dimension**: tech-debt (doc rot) / renderer
- **Location**: `docs/engine/memory-budget.md:179` ("…layer height handles with its std430 stride padding"); `docs/engine/shader-pipeline.md` and `docs/engine/exal.md` (no mention of `height_blend_band` / `byroTerrainHeightBlend` / `render.terrain_height_blend_band`)
- **Changed in**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (commit ea45af067) without the matching doc edits
- **Status**: NEW (same class as closed #5172 / #4337, `GpuTerrainTile` growth left docs stale; the struct size did not change this time)
- **Description**: The 4-byte tail at @204 is now a live per-tile value broadcast every frame. memory-budget.md still calls it padding. The renderer skill asks that "the rustdoc and shader-pipeline.md carry the new meaning" for semantic lane changes, but only the rustdoc does.
- **Suggested Fix**: Update memory-budget.md:179 to name `height_blend_band`. Add a short height-blend paragraph (setting, broadcast, consumers, band 0 = off) to shader-pipeline.md or exal.md next to the splat description.

## 4. Missing tests

| Changed path | Gap |
|---|---|
| `bindings.glsl::byroTerrainHeightBlend` (ea45af067) | No CPU-mirror or shader-text test of the invariants the doc comment claims: shares sum to 1, non-height lanes keep their exact share, `< 2` height lanes → untouched, band 0 → untouched. No continuity test (would have caught -01). |
| `VulkanContext::set_terrain_height_blend_band` | No test that it dirties the tile SSBO only on change and that a freshly allocated tile (band 0.0) is updated on the next frame. |
| `commands/terrain.rs` `TerrainBlendCommand` + `census` | No unit tests (parse error, out-of-range rejection, census tallies for Ready/Pending/Failed/not-requested). |
| `cell_loader/terrain.rs` `request_heights` on tile-less cells (2975d2906) | No test that a BTXT-only cell queues its base map, or that `apply_ready_heights` skips tile-less entities. |
| `core/settings/builtin.rs` new slider | Generic registry tests only; no assertion of the id/default/range. Low value, listed for completeness. |
| #6196 acceptance | No frame-time delta recorded for the extra up-to-9 BC4 fetches per terrain fragment and per ground-coupled blade fragment (`groundcover_blade.frag:129-136`). The issue requires one; not measurable here (no engine launch). |

## Not checked

- Visual confirmation of -01/-02 and any frame-time cost: needs the engine, a Vulkan device and game data (suite rule: no engine launch). -01 is shown analytically with a CPU mirror of the shader function, not on screen.
- `installed_fo4_worldspaces_resolve_their_climate` (`--ignored`, needs FO4 data and parses the full ESM); not run.
- Release-path `BYRO_VALIDATION=1` run for the new per-frame tile SSBO dirtying: needs the engine.

Next step: `/audit-publish docs/audits/AUDIT_INCREMENTAL_2026-10-20.md`
