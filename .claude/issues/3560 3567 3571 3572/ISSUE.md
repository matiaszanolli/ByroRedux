===== 3560 =====
OPEN | bug medium tech-debt 
# RT-14: the runtime audit harness silently mis-attributes telemetry between games — `kill -INT` on the `xvfb-run` wrapper leaves the engine alive on port 9876

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-14. **Reproduced live during this sweep.**

## Description

The `/audit-runtime` skill's documented teardown (`kill -INT $PID` on the backgrounded `xvfb-run ...` job) kills the **wrapper**, not the engine. `xvfb-run` execs the binary as a child, so the engine keeps running and keeps holding port 9876.

The FNV run that followed Oblivion therefore connected to the **still-live Oblivion engine** and captured Oblivion's numbers — `Entities: 718`, Oblivion's exact 8-path `tex.missing` list — under the FNV filename. The only tell was `dbg up at 1s`, impossible for a cell that takes ~40 s to load.

## Impact

This is exactly the RT-1/#1619 mis-attribution the skill warns about, but reached through **teardown failure rather than parallelism** — so running serially, as the skill instructs, does **not** prevent it. Any past `--game all` sweep using the documented teardown may carry silently shifted telemetry, including baselines regenerated from such a sweep.

## Fix applied for this audit (recommend folding into the skill)

1. Pre-flight assert `pgrep -x byroredux` is empty and port 9876 is unbound. **Note**: `pgrep -f 'target/release/byroredux'` self-matches the harness shell — use `pgrep -x`.
2. Resolve the real engine PID with `pgrep -x byroredux` **after launch**, and sweep any survivor after teardown.
3. **Cross-check** `Entities:` from the `byro-dbg` `stats` line against `entities=` on the `bench:` line, and hard-fail on divergence.

All five captured runs in the 2026-08-30 report pass that cross-check; the one run that failed it (the first FNV attempt) was discarded and re-run, not reported.

## Suggested Fix

Fold the three steps above into `.claude/commands/audit-runtime/SKILL.md` as mandatory pre-flight / post-flight / per-capture assertions, replacing the current `kill -INT $PID` teardown text.

## Completeness Checks
- [ ] **SIBLING**: Every other smoke test / harness that backgrounds `xvfb-run` and later signals `$!` audited for the same wrapper-vs-child gap (`docs/smoke-tests/*.sh`)
- [ ] **TESTS**: The cross-check (item 3) is what makes the failure *visible* rather than silent — it must be in the harness, not just the skill prose

===== 3567 =====
OPEN | bug renderer medium game:oblivion nifal 
# REN-2026-08-30-D6-01: the Oblivion `APPLY_HILIGHT2` normal-map alpha is consumed as BOTH parallax height and the normal-alpha-as-spec mask — the render-side predicate never consults `Material::parallax_height_in_alpha`

- **Severity**: MEDIUM
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs` (`normal_alpha_spec_binding_applies`, `normal_alpha_spec_applies`), `byroredux/src/render/static_meshes.rs` (`build_static_mesh_draws`, lines ~306-312 and ~474-484), `crates/nif/src/import/material/legacy_properties.rs` (the `APPLY_HILIGHT2` arm)
- **Status**: OPEN — new (the `parallax_height_in_alpha` field landed in `19813460`, after the 2026-08-27 sweep)
- **Description**: #3530 resolved a per-game channel-meaning decision at the NIFAL boundary: `Material::parallax_height_in_alpha` records that this material's height values live in the bound texture's **alpha**, because Oblivion ships no `_p.dds` and `legacy_properties.rs` therefore binds the *normal* map into `MaterialTextureSet::height`. The render path transports that as `PARALLAX_ALPHA_HEIGHT_BIT` on `parallax_map_index`.

  Fifty lines further down in the same loop, `normal_alpha_spec_binding_applies` makes an *independent* claim about the same channel of the same texture — that the normal map's alpha is a per-pixel **specular-intensity mask** — and re-points the gloss slot at the normal map with `NORMAL_ALPHA_SPEC_BIT`. It reads `material_kind`, `normal_has_alpha`, `normal_map_index` and `gloss_map_index`; it does **not** read `parallax_height_in_alpha`. The two are not mutually excluded anywhere.

  For an `APPLY_HILIGHT2` mesh the preconditions of the second predicate are satisfied by construction: `normal_has_alpha` must be true (that alpha *is* the height payload), `normal_map_index != 0` (the parallax slot was bound from it), and `material_kind < 100` for ordinary Oblivion architecture. Only a bound `NiTexturingProperty.gloss_texture` (`gloss_map_index != 0`) suppresses it.
- **Evidence**:
  - `legacy_properties.rs`: `if tex_prop.apply_mode == APPLY_HILIGHT2 && info.parallax_map.is_none() { … info.parallax_map = Some(normal); info.parallax_height_in_alpha = true; }`
  - `crates/nif/src/import/material/mod.rs:1249` — `height: self.parallax_map`, so `textures.height` and `textures.normal` resolve to the *same* path and therefore the same bindless handle.
  - `static_meshes.rs`: `if parallax_map_index != 0 && mat.is_some_and(|m| m.parallax_height_in_alpha) { parallax_map_index |= PARALLAX_ALPHA_HEIGHT_BIT; }`
  - `static_meshes.rs`: `if normal_alpha_spec_binding_applies(mat, normal_has_alpha, material_kind, metalness, normal_map_index, gloss_map_index) { gloss_map_index = normal_map_index | NORMAL_ALPHA_SPEC_BIT; }` — no `parallax_height_in_alpha` term.
  - `normal_alpha_spec_applies` body is exactly `material_kind < 100 && normal_map_index != 0 && gloss_map_index == 0`.
  - Both consumers then read the same texel: `material_sampling.glsl::sampleParallaxHeight` returns `texel.a`, and `triangle.frag:1247-1255` does `normalAlphaSpecMask = glossTexel.a; specStrength *= normalAlphaSpecMask;`.
  - `normal_has_alpha` originates from `dds::format_has_alpha` on the bound normal (`scene/nif_loader.rs:1101`), so it is true precisely for the population that carries height data.
- **Impact**: On the `APPLY_HILIGHT2` population (the commit message cites 1,433 properties across 741 vanilla Oblivion meshes) the specular strength is multiplied by the **height field**: crevices go matte and raised brickwork goes glossy, with the modulation tracking displacement rather than any authored spec mask. Symmetrically, the engine now asserts two mutually exclusive meanings for one channel in one draw with nothing arbitrating — which is the exact class of render-time channel-meaning re-derivation NIFAL exists to eliminate, reintroduced one predicate away from the field that was added to prevent it. Confined to Oblivion; every other producer leaves `parallax_height_in_alpha` false and is unaffected.
- **Suggested Fix**: Make the two exclusive at the canonical boundary rather than in the draw loop. Thread `parallax_height_in_alpha` into `normal_alpha_spec_applies` (or add it to `normal_alpha_spec_binding_applies`'s inputs, which already takes `Option<&Material>`) and return `false` when it is set, so a material whose normal alpha was already claimed as height cannot also claim it as a spec mask. Pin it with a test alongside the existing `normal_alpha_spec_binding_applies` cases in `material_translate.rs:1743-1770`. Before landing, census `NiTexturingProperty.gloss_texture` fill on the `APPLY_HILIGHT2` meshes to confirm the suppressing `gloss_map_index != 0` arm is as rare as it appears (the fix is correct either way; the census only sizes the affected population).

---
- **Cross-dimension corroboration**: Found independently three times — also filed as *D2-02* (SSBO/indexing) and *D19-02* (tangent-space). All three traced the same two predicates and reached the same conclusion; the write-up below is the NIFAL-dimension one, which carries the corpus figure.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D6-01

## Population census (added at publish time)

A sibling audit measured `Material::parallax_height_in_alpha` as true on **0 of 35,322**
vanilla Oblivion meshes (0 of 1,430 `APPLY_HILIGHT2` properties carry a normal/bump slot),
so the channel collision has **no live population on shipped Oblivion content today**.
File/fix it as an arbitration-correctness change; there is no visual repro to chase.

Related: the missing alpha-presence gate on the same `#3530` route is filed separately
(see the `D19-01` issue from this same report).


## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

===== 3571 =====
OPEN | bug renderer medium 
# REN-2026-08-30-D10-02: the #3308 comparison gate can only be run *before* the conversion — `analyze_depth_field` is hardcoded to the conventional mapping in both its background test and its decode

- **Severity**: MEDIUM
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/core/src/ecs/components/camera.rs` (`analyze_depth_field` L317, `linear_distance_from_depth` L277, cleared test at L351); `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`)
- **Status**: New
- **Description**: `DEFAULT_RENDER_DISTANCE`'s doc block states the gate's
  contract as *"Run it before the conversion, run it after, and the far
  decades' `distinct_codes` are the before/after evidence — the thing that was
  otherwise unobservable and that made shipping reversed-Z speculative."*
  `depth_capture.rs`'s module doc repeats it (*"after a reversed-Z conversion —
  report the before/after difference"*), as does `commands/depth.rs`. The code
  cannot deliver the "after" half. Three separate sites are hardwired to the
  conventional near→0 / far→1 mapping, and there is no mapping selector on
  `analyze_depth_field`:
  1. the background classifier `if z >= 1.0 { stats.cleared += 1; continue; }`
     (L351) — under reversed-Z the clear value is `0.0`, so *nothing* would be
     classified as background and the frame's entire sky would decode into the
     bands, swamping exactly the far decade the gate reads;
  2. the decode `linear_distance_from_depth` (L277), whose
     `denom = 1.0 - z * (f - n) / f` inverts only the conventional
     `z_ndc(d) = f/(f-n)·(1 − n/d)` — there is no
     `linear_distance_from_depth_reversed` sibling to the
     `depth_resolution_at_reversed` that *was* added;
  3. `DepthBand::analytic_resolution` is always populated from
     `self.depth_resolution_at(mid)` and
     `analytic_resolution_reversed` always from the reversed sibling — after a
     conversion the two columns are swapped relative to reality, so the
     `depth.stats` table's "BU/step (reversed-Z would be)" header is then wrong
     in both columns.
- **Evidence**: read the full body of `analyze_depth_field`
  (`camera.rs:317-390`) — it takes only `&self` and `&[f32]`; `Camera` carries
  no reversed/conventional flag, and grepping `_reversed` in that file yields
  only `depth_resolution_at_reversed` and `DepthBand::analytic_resolution_reversed`
  (both analytic-only). `depth.rs`'s `execute` calls
  `camera.analyze_depth_field(&capture.samples)` with no mapping argument.
- **Impact**: The gate is half a gate. Its stated reason for existing is to
  make the reversed-Z conversion non-speculative by giving it a measurable
  before/after; whoever does that work will find the "after" run reports
  `cleared = 0`, a wildly inflated last-decade sample count, and nonsense
  `BU/step` columns, and will have to fix the analysis in the same change that
  they are trying to validate — precisely the position #3308 is trying to avoid
  being in. This is a design gap in brand-new code, not a live rendering bug.
- **Suggested Fix**: Add a mapping discriminant (a `DepthMapping::{Conventional,
  Reversed}` enum parameter on `analyze_depth_field`, or a `reversed: bool`
  field on `Camera` set by whatever sets the projection) and route all three
  sites through it: cleared test becomes `z >= 1.0` / `z <= 0.0`, decode picks
  between `linear_distance_from_depth` and a new reversed inverse
  `d = n / (z·(1 − n/f) + n/f)`, and the two `analytic_*` columns are labelled
  "current mapping" / "other mapping" rather than fixed. Adding the reversed
  inverse also lets `depth_decode_round_trips_the_projection` cover the
  reversed encode, which today it cannot.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D10-02

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

===== 3572 =====
OPEN | bug renderer medium pipeline 
# REN-2026-08-30-D13-01: TAA resolves only the pre-composite direct HDR — sky, denoised indirect, volumetrics, caustics and bloom bypass the resolve entirely (FSR, the default, does not)

- **Severity**: MEDIUM
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs` (`record_post_passes`, lines 264–271: `record_taa_pass` → `record_composite_pass` → `record_bloom_pass` → `record_upscale_pass`); `crates/renderer/shaders/composite.frag` (`has_surface` / `is_sky`, lines 536–537); `crates/renderer/src/vulkan/taa.rs` (`TaaPipeline::write_descriptor_sets`, `curr_hdr` ← `hdr_views[f]`)
- **Status**: OPEN — architectural, root cause of the symptom `#2760` patched around from inside the constraint
- **Description**: The TAA resolve is wired to `composite.hdr_image_views[f]` — the **raw** main-render-pass HDR attachment, i.e. direct lighting only — and writes `history[f]`, which `composite` then samples as binding 0 (`hdrTex`, `composite.frag:497`). Everything composite *adds* after that point is never seen by TAA: the analytically-synthesised sky (`compute_sky`), the SVGF-denoised indirect, volumetrics, water caustics, and (since `#2796`) bloom. The FSR path is the mirror image: `record_upscale_pass` takes `scene_color = composite.scene_image(frame)` (`post_passes.rs:967–971`) — the fully composited, post-bloom scene — so FSR 3.1 temporally resolves *all* of it. Since `UpscalerMode::default()` is `Fsr3(Quality)` (`upscaling.rs`), the lower-coverage path is the one selected by `--upscaler taa`.
  The concrete visible consequence is the geometry/sky silhouette. `composite.frag:536` classifies each pixel with a hard binary `depth < 1.0` against the **jittered** depth buffer, downstream of the resolve. Sub-pixel Halton jitter flips which side of a silhouette a pixel centre lands on every frame, so that pixel alternates between "TAA-resolved geometry colour" and "freshly-computed, never-temporally-filtered sky", and no filter in the chain ever averages the two. `#2760` correctly diagnosed the mechanism in `taa.comp`'s own comment ("sky is synthesised later in composite.frag and never exists in this HDR attachment") and softened the *history-acceptance* half of it (`disocclusionFromSky` → `gamma = 0.0` box filter) — but it can only improve the frames on which the pixel is geometry. The frames on which jitter makes the pixel sky bypass `hdrTex` for the sky branch entirely.
- **Evidence**:
  - `post_passes.rs:264` `self.record_taa_pass(cmd, frame);` then `:266` `self.record_composite_pass(cmd, frame);` — TAA strictly precedes composite.
  - `taa.rs::write_descriptor_sets`: `curr_hdr = ... .image_view(hdr_views[f])`, and `resize.rs:968–971` / `init.rs:1287–1338` show `hdr_views` is `composite.hdr_image_views` (the render-pass colour attachment), while composite's binding 0 is rebound to `taa.output_view(i)` at `init.rs:1338` / `resize.rs:1001` / `resize.rs:1372`.
  - `post_passes.rs:967–971`: `let scene_color = self.composite... .scene_image(frame);` handed to `FrameUpscaler::record` — post-composite, and `record_bloom_pass` (`:895–903`) has already written into that same `scene_images[frame]`.
  - `composite.frag:535–537`: `float depth = texelFetch(depthTex, ivec2(gl_FragCoord.xy), 0).r; bool has_surface = depth < 1.0; bool is_sky = !has_surface && ...` — no temporal term, no coverage term.
  - `taa.comp:150–165` (the `#2760` comment block) states the constraint explicitly.
- **Impact**: On `--upscaler taa`, exterior geometry/sky silhouettes crawl every frame with a parked camera — the exact artefact TAA exists to remove — and indirect/volumetric/caustic edges receive no temporal antialiasing at all. Because the default upscaler hides it, the TAA fallback path (FSR context creation failure at startup promotes to `UpscalerMode::Taa`, per `upscaling.rs`'s `Taa` doc / `#2480`) silently delivers materially worse image stability than the mode it replaced, in a way no `cargo test` observes.
- **Suggested Fix**: Move the TAA resolve to the same tap FSR uses — dispatch it on `composite.scene_images[frame]` after `record_composite_pass` (and after or before bloom, matching FSR's tap) instead of on the raw HDR attachment, so one code path feeds both temporal reconstructors the same image. That makes `#2760`'s `disocclusionFromSky` special case unnecessary rather than merely mitigated, since the sky would then be present in the resolved image on both jitter phases. **Needs RenderDoc verification** for the resulting layout/barrier sequence (`scene_images` is `COLOR_ATTACHMENT | SAMPLED | TRANSFER_SRC | STORAGE` and already changes layout twice in the tail of the frame) — do not ship the barrier reshuffle on test evidence alone. If the move is judged too large, the narrower fix is to feed composite a temporally-stable sky/geometry coverage instead of the binary `depth < 1.0` (e.g. resolve coverage in TAA and pass it through), but that is a second temporal history to validate.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D13-01

> **Policy note (publish-time):** per the project's standing rule, no speculative Vulkan render-pass / pipeline / barrier restructure is proposed here. The observation is filed with its evidence; any scope change needs a `BYRO_VALIDATION=1` sync-validation run or a RenderDoc capture first.


## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

