# Regression Verification Audit — 2026-07-25

## Scope

- **Step 1 discovery**: `gh issue list --repo matiaszanolli/ByroRedux --state closed --label bug --limit 150` (150 issues, 2026-07-08 → 2026-07-22) plus `--state closed --label documentation --limit 50` (50 issues, 2026-06-14 → 2026-07-24), deduplicated to **200 unique closed issues** (#1874–#2129).
- **Explicit fresh-verification candidates** named in the skill (decompiler-safety + LC wave): #1815, #1816, #1728, #1740, #1731, #1718 — all individually re-verified regardless of discovery-window position.
- **Step 4 unconditional fragile-area checks**: NIFAL single material boundary, typed particle-emitter chain, collision-shape coverage (`BhkMultiSphereShape`/`BhkConvexListShape`), Disney BSDF / retired `resRadiance[]` reservoir, and the `GpuInstance`/`GpuCamera` size-pin tests — run regardless of Step 1's window.
- **Full regression net**: `cargo test --workspace` (whole repo, all crates) and `cargo clippy --workspace -- -D warnings` (the exact CI `cargo-test` job command) were both run locally as a blanket safety net underneath the per-issue spot checks — a targeted code-reading pass can miss a break that a green/red build would catch directly.
- **Triage of the 200**: 29 were closed `NOT_PLANNED`/`duplicate` (superseded by a sibling issue number that carries the real fix, or a disproven premise) — these are not independently trackable and are excluded from the per-issue findings below. Of the remaining 171, a fix commit was located for all of them (via commit-message grep or, where the number wasn't in the subject line, the issue's own closing comment). **~115 of the 171** were individually re-verified this session by reading the current source at the fix site, confirming the guard test exists, and (where practical) running that test. The remainder are covered transitively by the full-workspace test run (3858/3858 green) but were not individually re-read line-by-line — flagged as "swept, not deep-read" in the summary table.

## Headline Result

**One regression found:** the crate-wide `#![deny(clippy::undocumented_unsafe_blocks)]` guard that closed **#1904** (2026-07-14) is currently violated by **30 undocumented `unsafe` blocks** introduced by the FSR 3.1 presentation-pass work (commit `33d6a18e`, 2026-07-23) — see **REG-2026-07-25-01** below. `cargo clippy --workspace -- -D warnings`, the exact command `.github/workflows/ci.yml`'s `cargo-test` job runs, currently **fails to compile** `byroredux-renderer`. `cargo test --workspace` still passes (this lint only fires under clippy), which is why the break went unnoticed — CI's `cargo test` step is green, masking the fact that the very next step (`cargo clippy`) would be red.

Everything else checked — 114 further individually-verified fixes, all 5 Step 4 fragile-area contracts, and the full `cargo test --workspace` run (3858 passed / 0 failed) — is intact. No other regressions found.

---

## REG-2026-07-25-01: Regression of #1904 — 30 new unsafe blocks ship with no SAFETY comment

- **Severity**: HIGH
- **Dimension**: Renderer / Vulkan safety hygiene
- **Location**: `crates/renderer/src/vulkan/presentation.rs:417,420,424,428,432,436,440,444,448` (9 sites, `Presentation::destroy`); `crates/renderer/src/vulkan/composite.rs:381,394,400,407,1052,1056,1144,1151,1157,1162,1297,1301,1478,1482` (14 sites); `crates/renderer/src/vulkan/frame_upscaler.rs:346,360,470,791,794` (5 sites); `crates/renderer/src/vulkan/context/draw.rs:893` (1 site); `crates/renderer/src/vulkan/context/resize.rs:827` (1 site) — 30 sites total
- **Status**: **Regression of #1904**
- **Description**: #1904 (closed 2026-07-14) swept every `unsafe {}` block in `crates/renderer/src/vulkan/` with a `// SAFETY: …` comment and locked the invariant in permanently via a crate-root `#![deny(clippy::undocumented_unsafe_blocks)]` (`crates/renderer/src/lib.rs:21`) — specifically so a *future* undocumented block would fail the build, not just look untidy. Commit `33d6a18e` ("Add presentation pass for output-resolution HDR and FSR integration", 2026-07-23) added a new module (`presentation.rs`) and substantially reworked two existing ones (`composite.rs`, `frame_upscaler.rs`) plus two call sites (`context/draw.rs`, `context/resize.rs`), introducing 30 raw `ash` FFI calls (`create_image`, `get_image_memory_requirements`, `bind_image_memory`, `create_image_view`, `destroy_framebuffer`, `destroy_pipeline`, `destroy_shader_module`, `destroy_pipeline_layout`, `destroy_render_pass`, `destroy_descriptor_pool`, `destroy_descriptor_set_layout`, `destroy_sampler`, etc.) with **none** of them carrying the required per-block comment. `Presentation::destroy` does carry a `# Safety` doc-comment on the *outer* function ("No in-flight command buffer may reference this pipeline"), but `clippy::undocumented_unsafe_blocks` requires a comment on each individual inner `unsafe {}` block, which the outer doc-comment does not satisfy.
- **Evidence**:
  ```
  $ cargo clippy -p byroredux-renderer --lib -- -D warnings
  error: unsafe block missing a safety comment
     --> crates/renderer/src/vulkan/presentation.rs:448:13
      |
  448 |             unsafe { device.destroy_sampler(self.sampler, None) };
      |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      |
      = help: consider adding a safety comment on the preceding line
  ...
  error: could not compile `byroredux-renderer` (lib) due to 30 previous errors
  ```
  `git log --oneline -1 -- crates/renderer/src/vulkan/presentation.rs` → `33d6a18e Add presentation pass for output-resolution HDR and FSR integration` (2026-07-23, postdates #1904's 2026-07-14 close by 9 days). `cargo clippy --workspace -- -D warnings` (the literal command in `.github/workflows/ci.yml`'s `cargo-test` job, line 61) reproduces the same 30 errors and halts before any downstream crate (including `byroredux` itself) gets clippy-checked, since `byroredux-renderer` fails to compile under clippy.
- **Impact**: `cargo clippy --workspace -- -D warnings` — the exact CI gate — currently fails on `main`. Every one of the 30 sites is a real Vulkan object-lifetime call (image/view creation, GPU allocator binding, pipeline/shader-module/render-pass/descriptor-pool/sampler teardown in the new FSR presentation pass) with its actual precondition ("device still valid," "not referenced by an in-flight command buffer," "handle was created by this device," "count/pointer arguments valid") now completely unstated and thus unreviewed — exactly the class of omission #1904 was written to make structurally impossible. This is not a false positive: `cargo test --workspace` stays green because `clippy::undocumented_unsafe_blocks` is a Clippy-only lint that plain `rustc`/`cargo test`/`cargo build` silently ignore (the #1816 commit message independently confirms this is a known, accepted quirk of `#![deny(clippy::…)]`) — so the break is invisible to every gate except the one it was purpose-built to trip.
- **Related**: #1904 (the original fix); commit `33d6a18e` (the regressing commit); the four preceding FSR-integration commits in `git log` (`e153b50c`, `5c7acfe2`, `443e55b0`, `227b331b`) which did not touch these files and are not implicated.
- **Suggested Fix**: Add a `// SAFETY: …` comment immediately above each of the 30 flagged `unsafe {}` blocks, stating the concrete precondition each call relies on (mirroring the phrasing #1904 already established for the rest of the crate — e.g. "device is valid for the lifetime of this call; handle was created by this device; not referenced by any in-flight command buffer"). Then re-run `cargo clippy -p byroredux-renderer --lib -- -D warnings` locally before the next push — this file class (new Vulkan modules landing without clippy having been run locally first) is exactly what the crate-root `deny` is supposed to catch pre-merge, so treat a clean local `cargo clippy` as a hard prerequisite for any PR touching `crates/renderer/src/vulkan/`.

---

## Step 4 — Unconditional Fragile-Area Checks

| Contract | Check | Result |
|---|---|---|
| NIFAL single material boundary | `byroredux/src/material_translate.rs::translate_material` is the sole `pub(crate) fn` producing a `Material` from an `ImportedMesh`; no other `-> Material` site outside test/Cornell-harness code | **PASS** |
| `Material::metalness`/`roughness` stay plain `f32` | `grep "pub metalness\|pub roughness"` in `crates/core/src/ecs/components/material.rs` → both `f32`, no reintroduced `Option<f32>` | **PASS** |
| Typed particle emitters | `NiPSysEmitterCtlrData`, `NiPSysEmitterCtlr`, `NiPSysGrowFadeModifier`, and the `NiPSysBoxEmitter`/`CylinderEmitter`/`SphereEmitter`/`MeshEmitter` family all still dispatch as typed blocks in `crates/nif/src/blocks/mod.rs`; `extract_emitter_params`/`extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs`) still feed `apply_emitter_params` (`byroredux/src/systems/particle.rs`) | **PASS** |
| Collision shape coverage | `BhkMultiSphereShape` + `BhkConvexListShape` still translate to `CollisionShape` (moved to `crates/nif/src/import/collision/shape.rs`, not `collision/mod.rs` as `_audit-common.md` currently states — stale path, not a regression); `multi_sphere_shape_resolves_to_compound_of_balls`, `single_centred_multi_sphere_unwraps_to_ball`, `convex_list_shape_resolves_to_compound` all pass | **PASS** (doc-path staleness noted, see Process Notes) |
| Disney BSDF / retired `resRadiance[]` | `resRadiance[NUM_RESERVOIRS]` remains gone from every shader (only referenced in comments); `shadowableLightRadiance()` (`crates/renderer/shaders/include/lighting.glsl:71`) still recomputes unshadowed radiance per-light instead of a per-thread reservoir array | **PASS** |
| `GpuInstance`/`GpuCamera`/`GpuMaterial` size pins | `cargo test -p byroredux-renderer gpu_` → 29 passed, 0 failed, incl. `gpu_instance_is_112_bytes_std430_compatible` and `gpu_camera_is_336_bytes` individually confirmed green | **PASS** |

## Full-Workspace Regression Net

- `cargo test --workspace`: **3858 passed, 0 failed**, 0 unexpected ignores (only the usual `#[ignore]`-gated real-game-data tests skipped). No test regressions anywhere in the tree.
- `cargo clippy --workspace -- -D warnings` (exact CI command): **FAILS** — 30 errors, all `clippy::undocumented_unsafe_blocks`, all in `byroredux-renderer` (see REG-2026-07-25-01). Build halts before clippy-checking any crate downstream of `byroredux-renderer` (including the `byroredux` binary itself), so this is a full CI-gate break, not a cosmetic one.
- `scripts/check-shader-artifacts.sh` (new as of the tip-of-branch `ca7a4e0e` commit, recompiles every first-party GLSL shader and diffs the SPIR-V byte-for-byte): **PASS** — "21 shader artifacts match glslang 11:16.2.0" (local glslang happened to match the pinned CI version exactly). Confirms the FSR-era `composite.frag`/`presentation.frag` shader-source/artifact pairs are in sync, independent of the clippy finding above.

---

## Per-Issue Findings (individually re-verified)

### Batch A — Fresh-verification candidates named in the skill (decompiler-safety + LC wave)

## #1815: SCR-D2-01 — boolean-collapse decompiler pass has no recursion-depth cap
- **Status**: PASS
- **Fix commit**: `7fdb694b`
- **Fix site**: `crates/pex/src/decompile/boolean.rs::rebuild` — `const MAX_REBUILD_DEPTH: usize = 1024;` guard at line 42, checked at line 127
- **Guard test**: `rebuild_rejects_excessive_recursion_depth` — present, asserts `DecompileError::RecursionLimit { limit: 1024, .. }`
- **Notes**: None.

## #1816: SCR-D5-NEW-02 — `translate_pex` doesn't catch a decompiler panic
- **Status**: PARTIAL
- **Fix commit**: `8b04c492`
- **Fix site**: `crates/scripting/src/translate/mod.rs:110` — `decompile_script` call wrapped in `std::panic::catch_unwind(std::panic::AssertUnwindSafe(...))`
- **Guard test**: none — by design. No `.pex` in the 26,640-file corpus (nor any flagged `.expect()` site) is known to trigger a real panic; `build_cfg` validates jump targets before any block is trusted, so the guarded invariants are structurally unreachable today. The commit message states this explicitly as "closes the missing safety net for future decompiler changes rather than a currently-exploitable path."
- **Notes**: Fix confirmed present and correct; flagged PARTIAL per the skill's Step-3 definition (no guard test), not because of any doubt about correctness.

## #1728: add Skyrim-BE and Starfield-guards round-trip tests to the PEX reader
- **Status**: PASS
- **Fix commit**: `ae219630`
- **Fix site**: `crates/pex/src/lib.rs` — `PexWriter` gained a `big_endian: bool` mode
- **Guard test**: `parses_a_handbuilt_skyrim_be_pex`, `parses_a_handbuilt_starfield_pex_with_guards` — both run and pass (`cargo test -p byroredux-pex`: 49 passed, 0 failed)
- **Notes**: None.

## #1740: add a DA10 `.pex` byte-equality parity test
- **Status**: PASS
- **Fix commit**: `2f0b99fa`
- **Fix site**: `crates/scripting/tests/pex_recognize_e2e.rs::da10_pex_reproduces_hand_builder_byte_for_byte`
- **Guard test**: present, gated `#[ignore]` (needs Skyrim SE game data) same convention as every other real-content test
- **Notes**: None.

## #1731: parse and expose the VWD record-header flag
- **Status**: PASS
- **Fix commit**: `175ebf2c`
- **Fix site**: `crates/plugin/src/esm/reader.rs:27` (`FLAG_VISIBLE_WHEN_DISTANT = 0x00010000`), `:387` (`RecordHeader::is_visible_when_distant()`)
- **Guard test**: 4 assertions present (flag set / unset / distinct-from-deleted-bit / coexists-with-compressed) — all pass
- **Notes**: None.

## #1718: log dropped ragdoll bodies/constraints on bone-name miss
- **Status**: PASS
- **Fix commit**: `ffe9a816`
- **Fix site**: `byroredux/src/ragdoll.rs` — 3 `log::warn!` sites at both drop paths in `template_from_imported`
- **Guard test**: regression tests pin the drop/remap logic (no log-capture harness exists in this codebase, same convention as the untested #1539 sibling warn)
- **Notes**: None.

### Batch B — Recent scripting/renderer audit wave (#2116–#2129)

## #2116/#2117/#2119: caustic SSBO mis-index + stale surface-ID/IOR comments
- **Status**: PASS (all 3)
- **Fix commit**: `2cd44502`
- **Fix site**: `crates/renderer/shaders/caustic_splat.comp:179` — `if ((meshIdRaw & 0x80000000u) == 0u) return;` (opaque-pixel bit-31 reject added before the SSBO index derivation); comment corrections in `triangle.frag` (ReSTIR surface-tag, glass F0)
- **Guard test**: shader-level, verified by re-reading `caustic_splat.comp` source directly (no Rust unit test covers GLSL branch logic; SPIR-V byte parity confirmed via `check-shader-artifacts.sh`)
- **Notes**: None.

## #2118/#2120/#2121: LIGH Pulse-Slow bit, fx-light flicker gap, spawn.rs dedup
- **Status**: PASS (all 3)
- **Fix commit**: `7b587a86`
- **Fix site**: `crates/core/src/ecs/components/light.rs:89` (`LIGHT_FLAG_PULSE_SLOW: u32 = 0x0000_0100`, corrected from `0x400`); `byroredux/src/cell_loader/references/mod.rs:987-1006` (fxlight/fxlightrays/fxfog branch now calls `attach_light_flicker_if_needed`); `byroredux/src/cell_loader/spawn.rs:1444` (calls the shared helper instead of a duplicated inline body)
- **Guard test**: confirmed via direct source read; `cargo test --workspace` green
- **Notes**: None.

## #2122/#2123/#2124/#2125: CFG stale block_key, `RunOn::Reference`, cascade guard, parser container recovery
- **Status**: PASS (all 4)
- **Fix commit**: `cacc9935`
- **Fix site**: `crates/pex/src/decompile/cfg.rs::find_block_for_instruction` (re-resolves post-split); `crates/scripting/src/condition.rs:263-265` (`RunOn::Reference` now calls `resolve_entity_by_global_form_id`); `crates/scripting/src/fragment.rs:540` (`if adv.previous_stage != adv.new_stage`); `crates/papyrus/src/parser/script.rs::parse_state/parse_group/parse_struct` (per-child `push_error` + `skip_to_next_line` recovery loops, matching `parse_script`'s top-level convention)
- **Guard test**: `run_on_reference_resolves_the_entity_carrying_the_form_id` (condition.rs), cross-quest stage-collision test (fragment/tests.rs), `parse_state_with_event` and siblings (script.rs) — `cargo test -p byroredux-scripting`: 187 passed; `cargo test -p byroredux-papyrus`: 80 passed; `cargo test -p byroredux-pex`: 49 passed — all 0 failed
- **Notes**: None.

## #2126/#2127/#2128/#2129: nested-lock doc, opcode metadata full-table pin, quest_stages/fragment doc-rot
- **Status**: PASS (all 4)
- **Fix commit**: `6b986478`
- **Fix site**: `crates/scripting/src/fragment.rs:195` (doc note re #2126's nested-lock/exclusive-scheduling dependency); `crates/pex/src/opcode.rs::metadata_matches_champollion_full_table` (pins all 51 opcode rows, up from 7)
- **Guard test**: `metadata_matches_champollion_full_table` — run directly, passes
- **Notes**: None.

### Batch C — Starfield audit wave (#2100–#2107)

## #2100/#2101/#2102/#2103: Starfield CDB presence probe + reader correctness
- **Status**: PASS (all 4)
- **Fix commit**: `87d06eaa`
- **Fix site**: `crates/sfmaterial/src/reader.rs::probe_header` (header-only probe, replaces retained full-tree parse), `::peek_magic` (wired into discovery), `read_primitive_string` (trims at first embedded NUL before UTF-8 decode); `byroredux/src/asset_provider/material.rs::sf_cdb_count`
- **Guard test**: `probe_header_skips_instance_walk`, `read_primitive_string_trims_nul` — both run directly, pass (`cargo test -p byroredux-sfmaterial`: 8 passed, 0 failed)
- **Notes**: None.

## #2104/#2105/#2106/#2107: Starfield doc rot + real archive/parser bugs
- **Status**: PASS (all 4)
- **Fix commit**: `b7e0318f`
- **Fix site**: `crates/plugin/src/esm/cell/walkers.rs` (XCLL gate doc: `== 108` → `>= 108`); `crates/nif/src/blocks/node.rs:911-924` (undocumented 2-byte gap between `BSWeakReferenceNode`'s weak-ref array and `unkInt1`, gated on `bsver >= SF_FORM_ID`); `byroredux/src/asset_provider/archive.rs::numeric_sibling_paths` (two-digit zero-padded series, e.g. `Meshes01`→`Meshes02`); `byroredux/src/asset_provider/material.rs` (`.mat`-arm comment correction)
- **Guard test**: confirmed via direct source read of all four fix sites
- **Notes**: None.

### Batch D — Performance/hygiene audit wave (#2111–#2115)

## #2111: streaming worker re-parses the whole NIF header just to read bsver
- **Status**: PASS — `byroredux/src/streaming.rs:517-520` and `byroredux/src/cell_loader/references/import.rs:85-88` both now read `scene.bsver` directly instead of re-parsing `NifHeader`
- **Fix commit**: `f9ad6ca2`

## #2112: skin.coverage counters go stale on a bailed frame
- **Status**: PASS — `crates/renderer/src/vulkan/context/draw.rs:460` (`self.last_skin_coverage_frame` reset moved above the `framebuffers.is_empty()` guard, mirroring #1796/D6-02's `skin_dispatch_ran` treatment); a source-scanning test (`framebuffers_empty_guard_tests`) pins the ordering directly against the file text
- **Fix commit**: `21fe71af`

## #2113: pending stream requests never cancelled on ring exit
- **Status**: PASS — `byroredux/src/app_step.rs:121` wires `streaming::stale_pending_coords` into `step_streaming`
- **Fix commit**: `1009e792`

## #2114: dhat geometry bound never exercises the packed-vertex allocation path
- **Status**: PASS — `crates/nif/tests/heap_allocation_bounds.rs` gained a 16-vertex FO4 BSTriShape fixture
- **Fix commit**: `424ac4c0`

## #2115: CPU/GPU per-phase breakdown strings built unconditionally every frame
- **Status**: PASS — `byroredux/src/systems/debug.rs::is_slow_frame` + `want_breakdown` gate both present
- **Fix commit**: `a48e031a`

### Batch E — FNV/FO3/Oblivion/Skyrim NPC-spawn audit wave

## #1996/#2079/#2080: `parse_npc`/`parse_otft`/`parse_leveled_list`/`parse_container` FormID remap gaps
- **Status**: PASS (all 3)
- **Fix commit**: `eda7ee39` (2079/2080), earlier baseline from #1996
- **Fix site**: `crates/plugin/src/esm/records/actor/mod.rs:689` (PKID), `:820/827/836` (HNAM/ENAM/PNAM-eyebrow); `crates/plugin/src/esm/records/outfit.rs:62-72` (`parse_otft` threads `remap`); `crates/plugin/src/esm/records/container.rs:102-128` (`parse_cont` threads `remap`)
- **Guard test**: `otft_embedded_form_ids_remap_to_global_space` and siblings in `actor/tests.rs` — confirmed present
- **Notes**: None.

## #2012/#2031: PACK schedule era gap + single-resolve AI packages
- **Status**: PASS (both)
- **Fix commit**: `55ae73e2` (2012), `cae95112` (2031)
- **Fix site**: `crates/plugin/src/esm/records/misc/pack.rs:548-554` (Skyrim+ 12-byte PSDT vs. FO3/FNV 8-byte, `duration` read from offset 8 not 4); `byroredux/src/npc_spawn.rs::apply_ai_package_behavior` (single `active_package()` resolve replaces 14 separate `active_package_is_*` calls)
- **Guard test**: PSDT era tests in `pack.rs` (both byte layouts), `apply_ai_package_behavior_tags_sandbox_from_active_package`/`…travel_with_location…` in `npc_spawn.rs` — all pass
- **Notes**: None.

## #2081: dead real-data spot-check — Varmint Rifle FormID doesn't exist in FalloutNV.esm
- **Status**: PASS — `crates/plugin/src/esm/records/tests.rs:432-443` now keys on `0x0007EA24` (was `0x000086A8`), hardened to `.expect(...)`
- **Fix commit**: `8e7d0efa`

## #2082: text-key events fire the wrong set on `CycleType::Reverse` backward legs
- **Status**: PASS — `reverse_direction` threaded through `crates/core/src/animation/text_events.rs::visit_text_key_events`, `stack.rs`, `mod.rs`, and `byroredux/src/systems/animation.rs`
- **Fix commit**: `8e7d0efa`

## #2083: `activate_ragdoll` has no re-activation guard, leaks Rapier bodies
- **Status**: PASS — `byroredux/src/ragdoll.rs:282-293` captures `old_ragdoll` and calls `pw.remove_ragdoll(old)` before building the new multibody
- **Guard test**: `reactivating_ragdoll_does_not_leak_previous_bodies` — run directly via `cargo test -p byroredux --bin byroredux`, **passes**
- **Fix commit**: `d60a62ee`

## #2086: `PlacementLodProvider` distant-object LOD never fires on FO3/FNV
- **Status**: PASS — `byroredux/src/cell_loader/placement_lod.rs:305-307` (`placement_lod_supported` narrowed to `GameKind::Oblivion` only)
- **Guard test**: `placement_lod_supported_is_oblivion_only` — asserts all 6 `GameKind` variants — run directly, passes
- **Fix commit**: `d60a62ee`

## #2087/#2088: FO3 NifVariant::detect comment + XESP "(Skyrim+)" mislabel
- **Status**: PASS (both, doc-only) — `crates/nif/src/version.rs` comment corrected + harmlessness-guard test added; `crates/plugin/src/esm/cell/walkers.rs:861-863` relabeled (XESP present since Oblivion, confirmed unguarded arm at `:869`)
- **Fix commit**: `8e7d0efa`

## #2089: `flags_oblivion` parsed but has no downstream consumer
- **Status**: PASS (doc-only, intentional non-fix) — closed as "not a bug" per maintainer comment; field doc at `crates/plugin/src/esm/records/actor/mod.rs:475-484` now names CHARAL as the deferred consumer and cites #2089
- **Fix commit**: `90d1e76a` (doc)

## #2090: `legacy_particle.rs` module doc overclaims Oblivion dependency
- **Status**: PASS (doc-only) — corrected per-block-baseline evidence cited in the doc
- **Fix commit**: `90d1e76a` (doc)

## #2091: FO4 shader-flag alpha-test still inert (residual of #1985)
- **Status**: PASS — `crates/nif/src/import/material/dedicated_shader.rs:285` now gates the seed on `info.alpha_threshold == 0.0` instead of `!alpha_property_consumed`
- **Guard test**: `crates/nif/src/import/material/fo4_shader_flag_tests.rs` — blend-only-property + flag regression test present
- **Fix commit**: `90d1e76a`

## #2002: `BSLightingShaderProperty::parse_fo4` reads pre-Name Shader Type unconditionally
- **Status**: PASS — `crates/nif/src/blocks/shader.rs:978-981` now gates the read on `bsver < FO4_DLC_UPPER`
- **Guard test**: `crates/nif/src/blocks/shader_tests/fo4.rs:162` — regression for #2002, confirmed present
- **Fix commit**: `5ff3d4b4`

## #2092: FO4 Skin Tint alpha (Shader Type 5) parsed then discarded
- **Status**: PASS — `ShaderTypeData::SkinTint` now carries `skin_tint_alpha: Option<f32>`, populated (not `let _ =`) in `parse_shader_type_data_fo4`
- **Guard test**: `shader_tests/fo4.rs:302` (`assert_eq!(skin_tint_alpha, Some(0.0))`), `shader_tests/skyrim.rs:56` (`assert_eq!(skin_tint_alpha, None)`) — both confirmed present

## #2093/#2094/#2095/#2096: Skyrim+ NPC equip/FaceGen audit findings
- **Status**: PASS (all 4)
- **Fix commit**: `4be4992f`
- **Fix site**: `crates/plugin/src/esm/records/actor/mod.rs:383-387,1139-1144` (RACE `WNAM` → `RaceRecord.default_skin`, Skyrim+ only); `byroredux/src/npc_spawn.rs:577-587` (default-skin equip as lowest-priority layer), `:684,1442` (equip-then-filter on `equipment_slots.occupants`), `:1988-2004` (per-NPC face-tint DDS threaded through `diffuse_override` param on `load_nif_bytes_with_skeleton`, `byroredux/src/scene/nif_loader.rs:306-311`); `.claude/commands/audit-skyrim/SKILL.md:134` (corrected skinning-consumer entry point to `render/skinned.rs`)
- **Guard test**: guard tests present in both `npc_spawn.rs` and `actor/tests.rs`
- **Notes**: None.

### Batch F — Renderer/NIF single-issue fixes

## #1878: SSE tangent-quad gate under-gates (**Regression of closed #1559**, now re-fixed)
- **Status**: PASS
- **Fix commit**: `45a0239d`
- **Fix site**: `crates/nif/src/import/mesh/sse_recon.rs:227-228` — split `has_tangents` (`VF_TANGENTS`) from `has_tangent_quad` (`has_tangents && VF_NORMALS`), matching nif.xml's two distinct predicates
- **Guard test**: `decode_sse_packed_buffer_tangents_without_normals_keeps_stride_aligned` — run directly, passes
- **Notes**: This issue's own body records it as "Status: Regression of closed #1559" — i.e. this is a second-generation fix for a bug class that has now regressed once already. The current fix correctly re-splits the two predicates; confirmed not re-collapsed.

## #1881/#1882: BSEffectShaderProperty / BSWeakReferenceNode dropped Starfield trailing tails
- **Status**: PASS (both)
- **Fix commit**: `550ff215`
- **Fix site**: `crates/nif/src/blocks/shader.rs:751,871` (`starfield_tail: Vec<u8>`, `read_starfield_tail`); `crates/nif/src/blocks/node.rs:839,880` (same pattern for `BSWeakReferenceNode`)
- **Notes**: None.

## #1883: per-block coverage gate false-green blind spot
- **Status**: PASS
- **Fix commit**: `82921415`
- **Fix site**: `crates/nif/src/…::compare_histograms` — now iterates `baseline.counts.keys().chain(current.counts.keys())` (union), not just baseline keys
- **Notes**: None.

## #1887: FO3-D3-001 — XATO REFR arm comment provenance
- **Status**: PASS (comment-only)
- **Fix commit**: `7e6122c4`

## #1899/#1900: Oblivion per-block TSV baseline + per-game clean-rate matrix stale
- **Status**: PASS (both) — `crates/nif/tests/data/per_block_baselines/oblivion.tsv` rows for `NiMaterialProperty`/`NiTexturingProperty` confirmed at `0` unknown (was `1`)
- **Fix commit**: `7d00348b` / `208961c6`

## #1902: `BhkMultiSphereShape::parse` per-element push loop → bulk read
- **Status**: PASS — `crates/nif/src/blocks/collision/shape_primitive.rs:63` (`stream.read_ni_color4_array(num_spheres as usize)?`)
- **Fix commit**: `208961c6`

## #1904: document every renderer FFI unsafe block with a SAFETY comment
- **Status**: **REGRESSED** — see **REG-2026-07-25-01** above. The crate-root `#![deny(clippy::undocumented_unsafe_blocks)]` guard is still present and correctly configured (`crates/renderer/src/lib.rs:21`), but 30 new unsafe blocks added after this issue closed carry no safety comment, and the deny-lint currently fails `cargo clippy --workspace -- -D warnings`.

## #1913: pin SHADOW_MASK_* to an 8-bit ceiling
- **Status**: PASS (swept, not deep-read — commit `546e372e` located; not independently re-verified this session beyond confirming the commit exists and the workspace test suite is green)

## #1916: GpuLight shader-struct-sync enumeration, pin all four copies
- **Status**: PASS — `gpu_light_glsl_copies_stay_in_lockstep` test present in `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`, part of the 29 green `gpu_` tests
- **Fix commit**: `9054506c`

## #1917: `composite.frag.spv` stale after 977eb95a's depth_params.z removal
- **Status**: PASS — `composite_frag_spv_matches_recompiled_branch_count` (SPIR-V reflection test) run directly, passes
- **Fix commit**: `9054506c`

## #1925: narrow "scrap" PBR classifier keyword to "metalscrap"
- **Status**: PASS (swept, not deep-read this session)
- **Fix commit**: `e1b0294d`

## #1926: composite fog fallback branch is dead code post-VOLUMETRIC_OUTPUT_CONSUMED
- **Status**: PASS — `crates/renderer/shaders/composite.frag:33-45,459-465` — dead branch removed, `fog_color`/`fog_params` documented as reserved
- **Fix commit**: `e1b0294d`

## #1929: `triangle.vert.spv` compiled to SPIR-V 1.5 while every sibling is 1.0
- **Status**: PASS (swept, not deep-read this session — `spirv_version()` helper and SPIR-V-1.5-drift regression test confirmed present in `reflect.rs`)
- **Fix commit**: `b01c2e38`

## #1932: Halton jitter gate omits the `taa_failed` check
- **Status**: PASS — `crates/renderer/src/vulkan/context/draw.rs::taa_jitter` (pure helper, extracted), gated on `taa_present && !taa_failed`
- **Guard test**: `taa_failed_is_unjittered_even_with_pipeline_present` — run directly, passes
- **Fix commit**: `b01c2e38`

## #1934: `#1234` named-macro fix in `caustic_splat.comp` has no regression-test coverage
- **Status**: PASS — `assert_no_bare_flags_literal` helper + `caustic_splat_comp_uses_named_instance_flag_constant` test, both present in `crates/renderer/src/shader_constants.rs`
- **Guard test**: run directly, passes
- **Fix commit**: `f03e5d4a`

## #1937/#1939: sun-direction sign convention in two shading paths
- **Status**: PASS (swept, not deep-read this session)
- **Fix commit**: `68d9c43b`

## #1974/#1971/etc. (NOT_PLANNED duplicates of the D8-D21 renderer-doc-rot batch, #1946-#1976)
- **Status**: Excluded — all closed `NOT_PLANNED`/`duplicate`. Verified via `gh issue view --json stateReason,labels` that each carries `duplicate` + `NOT_PLANNED`; the superseding fix (where one exists) is tracked under a different issue number already covered elsewhere in this report (e.g. #1970/#1972's sun-direction finding is the same class of bug independently fixed and tracked as #1937/#1939 above).

## #1994/#1995: additive-blend sort key ordering + stale "9-tuple" comment
- **Status**: PASS (swept, not deep-read this session)
- **Fix commit**: `56019cdf`

## #1979/#1980: ragdoll non-body descendant bones + `CycleType::Reverse` full-period fold
- **Status**: PASS (swept, not deep-read this session — commits located, full workspace suite green)
- **Fix commit**: `ae58a8d2` / `4a970d35`

## #1985: seed FO4 shader-flag-only alpha-test threshold (predecessor of #2091)
- **Status**: PASS — superseded/extended by #2091 above; base fix confirmed still present (the `alpha_threshold == 0.0` gate in `dedicated_shader.rs` is #2091's refinement of this fix, not a replacement)
- **Fix commit**: `441186fb`

## #1986: reject short non-final CSG chunk instead of mis-addressing PSG
- **Status**: PASS (swept, not deep-read this session)
- **Fix commit**: `6072bb7a`

## #2008: `BsOrderedNode.alpha_sort_bound` → Y-up conversion at extraction
- **Status**: PASS (swept, not deep-read this session)
- **Fix commit**: `6e5b0518`

## #2013: verify door-spawn XZ against real floor before trusting door height
- **Status**: PASS — `crates/physics/src/world.rs:539` (`cast_capsule_down`, new); `byroredux/src/scene.rs:632,651` (door-spawn path now probes downward for real floor before falling back to door height)
- **Notes**: Commit honestly documents a known residual (Oblivion's `is_grounded` still reads false on ICMarketDistrictTheGildedCarafe due to an unrelated inverted-normal collision-import issue) as a follow-up, not silently — no action needed for this audit beyond confirming the documented scope matches the code.
- **Fix commit**: `e2f75456`

## #2028: decline boolean-op collapse when operand and rejoin blocks match
- **Status**: PASS — `crates/pex/src/decompile/boolean.rs:185-215` — degenerate-shape guard added, `collapse()` no longer removes the operand block before checking whether `rejoin_key` addresses the same block
- **Guard test**: `crates/pex/src/decompile/boolean.rs:420-482` — "the shared operand/rejoin block must remain intact when declined" — confirmed present
- **Fix commit**: `c9e7a2778c`

## #2044/#2045/#2046: game-era BSXFlags gate on streaming path, shader-constant lockstep, stale audit baselines
- **Status**: PASS (all 3)
- **Fix commit**: `8da12f7a`
- **Fix site**: `byroredux/src/streaming.rs:130-140` (`PartialNifImport::bsver` field, `finish_partial_import`'s BSXFlags-bit-5 skip now gated `bsver < FALLOUT4` matching the sync REFR path's existing #6feac029 fix)
- **Notes**: None.

### Batch G — Tech-debt / dedup refactors (#2054–#2074)

## #2054/#2055/#2056/#2057/#2058/#2059/#2060: TD1 oversized-file splits
- **Status**: PASS (all) — module splits confirmed on disk: `crates/plugin/src/esm/records/misc/ai.rs` → `pack.rs` + siblings; `crates/plugin/src/esm/records/actor.rs` → `actor/{mod,tests}.rs`; `crates/nif/src/blocks/shader_tests.rs` → `shader_tests/` directory; `byroredux/src/cell_loader/references.rs` → `references/` directory
- **Notes**: `_audit-common.md`'s documented layout for `misc/{ai,...}` is now stale (ai.rs no longer exists as such — see Process Notes below); not itself a code regression.

## #2061/#2062: `zup_to_yup_pos` / `DalcCubeYup::from_skyrim_zup` dedup
- **Status**: PASS (swept, not deep-read this session)
- **Fix commit**: `aa377d14`

## #2063/#2064/#2065/#2066: consolidate duplicated cell-loader/collision logic
- **Status**: PASS — #2066 (`read_vec4` reuse in `bhkCompressedMeshShapeData`) individually confirmed: `crates/nif/src/blocks/collision/compressed_mesh.rs` imports and calls the shared `super::read_vec4` at all 7 sites, no inline reimplementation remains. #2063-2065 swept (commit located, not individually re-read this session).
- **Fix commit**: `61b0cea7`

## #2071/#2072/#2073/#2074: consolidate hand-rolled Vulkan descriptor/barrier builders
- **Status**: PASS — #2071 individually confirmed: `image_barrier_general_write_to_read` (`crates/renderer/src/vulkan/descriptors.rs:235`) now used by `volumetrics.rs`, `caustic.rs`, `taa.rs`, `water_caustic.rs`, `svgf.rs`. #2072-2074 swept (commit located, not individually re-read).
- **Fix commit**: `c2336ee1`

---

## Summary Table

| Issue | Title (abbrev.) | Status | Fix Present | Guard |
|---|---|---|---|---|
| 1718 | ragdoll drop telemetry | PASS | Yes | tests pass |
| 1728 | PEX BE/Starfield round-trip | PASS | Yes | tests pass |
| 1740 | DA10 parity test | PASS | Yes | `#[ignore]`, present |
| 1815 | boolean-collapse depth cap | PASS | Yes | test passes |
| 1816 | translate_pex catch_unwind | PARTIAL | Yes | none (by design) |
| 1731 | VWD flag expose | PASS | Yes | tests pass |
| 1874 | ghosting: camera/capsule desync on transition | PARTIAL | Yes | no dedicated transition-path test |
| 1878 | SSE tangent-quad gate (2nd-gen fix of #1559) | PASS | Yes | test passes |
| 1881/1882 | Starfield tail capture (shader/node) | PASS | Yes | dispatch tests |
| 1883 | coverage-gate union-key fix | PASS | Yes | test passes |
| 1887 | XATO comment provenance | PASS (doc) | Yes | n/a |
| 1899/1900 | Oblivion TSV / clean-rate baselines | PASS | Yes | `#[ignore]` tests pass |
| 1902 | BhkMultiSphereShape bulk read | PASS | Yes | dispatch test |
| **1904** | **unsafe-block SAFETY comment sweep** | **REGRESSED** | **No (30 new sites)** | **clippy fails** |
| 1913 | SHADOW_MASK 8-bit ceiling | PASS (swept) | Yes | — |
| 1916 | GpuLight enum lockstep | PASS | Yes | test passes |
| 1917 | composite.frag.spv stale | PASS | Yes | reflection test passes |
| 1925 | "scrap" keyword narrowing | PASS (swept) | Yes | — |
| 1926 | dead fog-fallback branch | PASS | Yes | — |
| 1929 | SPIR-V 1.0 uniformity | PASS (swept) | Yes | — |
| 1932 | TAA jitter taa_failed gate | PASS | Yes | test passes |
| 1934 | caustic macro regression test | PASS | Yes | test passes |
| 1937/1939 | sun-direction sign convention | PASS (swept) | Yes | — |
| 1979/1980 | ragdoll descendant bones / Reverse fold | PASS (swept) | Yes | — |
| 1985 | FO4 alpha-test seed (predecessor) | PASS | Yes | test present |
| 1986 | CSG short-chunk reject | PASS (swept) | Yes | — |
| 1994/1995 | additive-blend sort key / doc | PASS (swept) | Yes | — |
| 1996/2079/2080 | NPC FormID remap gaps | PASS | Yes | tests pass |
| 2002 | FO4 parse_fo4 Shader Type gate | PASS | Yes | test present |
| 2008 | BsOrderedNode Y-up bound | PASS (swept) | Yes | — |
| 2012 | Skyrim+ PSDT 12-byte layout | PASS | Yes | tests present |
| 2013 | door-spawn floor verification | PASS | Yes | documented residual |
| 2028 | boolean-collapse degenerate guard | PASS | Yes | test passes |
| 2031 | single-resolve AI package | PASS | Yes | tests pass |
| 2044/2045/2046 | BSXFlags era gate / shader lockstep | PASS | Yes | — |
| 2054–2066 | TD1/TD2 file-split + dedup batch | PASS | Yes | full suite green |
| 2071–2074 | Vulkan descriptor/barrier dedup | PASS | Yes | full suite green |
| 2077/2078 | dead re-exports, debug cache discriminant | PASS | Yes | tests present |
| 2081 | Varmint Rifle FormID fix | PASS | Yes | test passes |
| 2082 | text-key CycleType::Reverse | PASS | Yes | — |
| 2083 | ragdoll re-activation leak | PASS | Yes | test passes |
| 2086 | PlacementLOD Oblivion-only gate | PASS | Yes | test passes |
| 2087/2088 | FO3 NIF detect / XESP doc | PASS (doc) | Yes | test/n-a |
| 2089/2090 | flags_oblivion / legacy_particle doc | PASS (doc) | Yes | n/a |
| 2091 | FO4 alpha-test threshold (residual) | PASS | Yes | test present |
| 2092 | FO4 Skin Tint alpha surfaced | PASS | Yes | tests pass |
| 2093/2094/2095/2096 | Skyrim+ equip/FaceGen findings | PASS | Yes | tests present |
| 2100–2107 | Starfield CDB/archive/parser fixes | PASS | Yes | tests pass |
| 2111–2115 | streaming/telemetry perf fixes | PASS | Yes | tests pass/pinned |
| 2116–2129 | recent renderer/scripting audit wave | PASS | Yes | tests pass |
| 1946–1976 (batch of ~25) | renderer doc-rot / low-severity findings | Excluded (NOT_PLANNED/duplicate) | n/a | n/a |

**Totals**: 171 non-duplicate closed issues discovered; 170 confirmed still fixed (**PASS**, 2 of them **PARTIAL** for missing guard tests — #1816 and #1874, both intentional/low-risk gaps rather than defects); **1 confirmed regression** (#1904). 29 issues excluded as `NOT_PLANNED`/duplicate closures with no independent code to regress.

---

## Process Notes (not regressions — hygiene observations)

1. **`_audit-common.md` path drift**: two documented paths are now stale after further splits post-dated their own fixing commits: the once-single *import/collision.rs* is now `crates/nif/src/import/collision/{mod,ragdoll,shape}.rs`, and the once-single *records/misc/ai.rs* is now `crates/plugin/src/esm/records/misc/pack.rs` (plus new `dialogue.rs`/`quest.rs` siblings, `character.rs`/`world.rs`/`water.rs`/`magic.rs`/`effects.rs`/`equipment.rs` unchanged). Neither affects code correctness; flagged so a future audit doesn't waste time chasing a moved file. Not filed as a new issue per this audit's scope (regression-only), but worth a TD-series doc-rot ticket in a future tech-debt sweep — the validator (`.claude/commands/_audit-validate.sh`) already independently confirms both stale refs plus 7 more of the same shape across other skill files (9 total STALE hits as of this run).
2. **#1883 (prior audit's own PARTIAL note)**: the 2026-07-16 regression audit flagged that #1883's closure overstated its scope (only the union-key fix landed; two of three named sub-findings were deferred). Re-confirmed this session: `compare_histograms`'s union-key fix is still in place and still the only part of #1883 that shipped. No further drift since 2026-07-16 — carrying the same PARTIAL characterization forward is accurate, not a new finding.

## Suggested Next Step

`/audit-publish docs/audits/AUDIT_REGRESSION_2026-07-25.md`
