# Oblivion Compatibility Audit — 2026-04-25

**Auditor**: Claude Opus 4.7 (1M context)
**Baseline commit**: `1ebdd0d` (post the renderer audit-publish run #639–#683)
**Reference report**: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`
**Scope**: Readiness to load and render content from The Elder Scrolls IV: Oblivion (NIF v20.0.0.5, BSA v103, TES4 ESM).
**Method**: Six parallel dimension audits (legacy-specialist, renderer-specialist, general-purpose) against the codebase + vanilla install at `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.

---

## Executive Summary

| Layer              | State (2026-04-25)                                                                                          |
|--------------------|-------------------------------------------------------------------------------------------------------------|
| **NIF v20.0.0.5 parser** | All regression guards holding. Block dispatch coverage now complete (every type called out in the 04-17 H-2 list has a parser). |
| **BSA v103 archive** | **100% extraction** on 147,629 / 147,629 files across all 17 vanilla Oblivion BSAs (21.1s release). The slash-command-file claim "decompression NOT WORKING" is **stale**. |
| **TES4 ESM** | Walker structurally sound — every prior-audit C/H regression closed (tes4.rs stub deleted, LIGH RGB resolved, group-header-size variant-aware, CREA/ACRE in MODL/REFR matchers, XCLW water height parsed). Two new HIGH gaps: WEAP/ARMO Oblivion DATA layout silently collapsed onto FO3/FNV schema. |
| **Rendering path** | All 04-17 critical/high regressions closed: blend pipeline cache fully Gamebryo-discriminating (post #576+), color space hands-off invariant holding, depth state dynamic-wired, glow/detail/gloss/dark/parallax slots reaching GPU (post #221, GpuInstance = 352 B). One open HIGH: NiStencilProperty stencil ref/mask discarded (#337). |
| **Real-data parse rate** | **95.21% (7647 / 8032)** — NOT the 100% ROADMAP/CLAUDE.md claim. 384 truncated, 1 hard fail. The C-1 OOM fix held; the H-1 parser additions reduced the gap from ~9% to ~5%, but did not close it. |
| **Cell → render** | Anvil Heinrich Oaken Halls already renders. The next interior with creatures/animated doors needs the KF importer's `NiSequenceStreamHelper + NiKeyframeController` path, which is still a stub. |

### Top blockers (priority order)

1. **[CRITICAL / Dim 5 O5-1]** ROADMAP.md and CLAUDE.md assert 100% Oblivion parse rate; **measured rate is 95.21%**. Doc-truth bug, not a behavior regression — `nif_stats` exit code is 1 (gate firing correctly). 384 NIFs still truncate; 1 hard-fails.
2. **[HIGH / Dim 5 O5-2]** 84 of the 384 truncated files trip on `check_alloc` rejecting bogus u32 counts (1.7 GB / 2.1 GB / etc) harvested from misaligned stream offsets — every block AFTER the failing one is dropped, median ~30 lost per file. Files that previously OOM-aborted are now silent data loss. Worst offenders: `crates/nif/src/blocks/particle.rs` (NiPSysBoxEmitter / NiPSysGrowFadeModifier / NiPSysSpawnModifier).
3. **[HIGH / Dim 5 O5-3]** 154 files truncate at the root NiNode "failed to fill whole buffer" — root-block under-consumption. Same shape as the 04-17 138-file class; H-1 parser additions didn't clear it. Suggests at least one shared parent (NiAVObject? NiObjectNET?) has a field-width discrepancy on a subset of v20.0.0.5 content.
4. **[HIGH / Dim 6 O6-N-01]** KF importer still has no `NiSequenceStreamHelper + NiKeyframeController` path. Every Oblivion door idle, creature idle, NPC walk cycle parses without error and produces zero `AnimationClip`s. Comment at `controller.rs:1835-1838` is explicit: "remains as a follow-up". Cross-cuts FO3/FNV.
5. **[HIGH / Dim 4 O4-01]** `NiStencilProperty` stencil_function/ref/mask/fail/zfail/pass discarded; only `is_two_sided` consumed. Pipeline hardcodes `stencil_test_enable(false)`. Oblivion gates (the painted-portal interiors), mirrors, scrying orbs render as opaque holes. Open issue #337.
6. **[HIGH / Dim 3 O3-N-01, O3-N-02]** `parse_weap` and `parse_armo` collapse `GameKind::Oblivion` into the FO3/FNV `match` arm; Oblivion DATA shapes differ. Every WEAP `value`/`weight`/`damage` and ARMO `armor`/`value`/`health`/`weight` field is wrong on Oblivion. No rendering impact today — breaks any future damage/economy/inventory consumer.
7. **[MEDIUM / Dim 6 O6-N-02, O6-N-03]** Doc-surface staleness:
   - `.claude/commands/audit-oblivion.md:19, 22` still asserts "decompression NOT WORKING".
   - `ROADMAP.md:63-64, 71, 102, 314` still claims BSA v103 decompression blocks Oblivion exterior.
   The actual blocker is the same as Skyrim/FO3 was: cell loader not wired to TES4 worldspace + LAND records. BSA opens fine.

---

## Regression Guard List (verified holding)

These fixes from prior audits have been re-verified against the current code:

| Guard | File:line | Status |
|-------|-----------|--------|
| `user_version` threshold ≥ 10.0.1.8 (older NetImmerse files have num_blocks at that position) | [crates/nif/src/header.rs:81](crates/nif/src/header.rs#L81) | ✓ holding (test `accept_netimmerse_header`) |
| BSStreamHeader dual condition `version == 10.0.1.2 \|\| user_version >= 3` | [crates/nif/src/header.rs:104-110](crates/nif/src/header.rs#L104-L110) | ✓ holding (test `bs_stream_header_not_read_for_off_spec_version`) |
| NiTexturingProperty raw u32 count, NO `Has Shader Textures: bool` gate | [crates/nif/src/blocks/properties.rs:363-364](crates/nif/src/blocks/properties.rs#L363-L364) | ✓ holding (regression test `:746` + 5 neighbours) |
| Pre-Gamebryo (v < 5.0.0.1) inline-type-name fallback at `log::debug!` | [crates/nif/src/lib.rs:221-247](crates/nif/src/lib.rs#L221-L247) | ✓ holding |
| No-block-size recovery uses runtime size cache, not `block_size`-driven advance | [crates/nif/src/lib.rs:212, 408-493](crates/nif/src/lib.rs#L212) | ✓ holding |
| u16 NiAVObject flags on Oblivion (bsver=11 ≤ 26) | [crates/nif/src/blocks/base.rs:78-82](crates/nif/src/blocks/base.rs#L78-L82) | ✓ holding |
| LIGH DATA color = RGB (NOT BGR — 04-17's premise was wrong; #389 reverted) | [crates/plugin/src/esm/cell.rs:1567-1613](crates/plugin/src/esm/cell.rs#L1567-L1613) | ✓ holding |
| `EsmVariant::Oblivion` 20-byte vs `Tes5Plus` 24-byte group header dispatch | [crates/plugin/src/esm/reader.rs:48-54, 485-487](crates/plugin/src/esm/reader.rs#L48-L54) | ✓ holding (test `group_content_end_is_variant_aware:951`) |
| CREA in MODL match arm + ACRE alongside REFR/ACHR | [crates/plugin/src/esm/cell.rs:601-604, 979-982](crates/plugin/src/esm/cell.rs#L601-L604) | ✓ holding (commit a8f21f9) |
| XCLW water height in interior + exterior CELL loops | [crates/plugin/src/esm/cell.rs:747-759, 1475-1482](crates/plugin/src/esm/cell.rs#L747-L759) | ✓ holding |
| NiTextKeyExtraData uses `allocate_vec` (was 135 GB OOM on `upperclassdisplaycaseblue01.nif`) | [crates/nif/src/blocks/interpolator.rs:711-733](crates/nif/src/blocks/interpolator.rs#L711-L733) | ✓ holding (regression test `parse_corrupt_text_key_count_returns_err:1063`) |
| Blend pipeline cache `(src, dst, two_sided)` covers all 11 Gamebryo AlphaFunction values | [crates/renderer/src/vulkan/pipeline.rs:46-54, 94-109](crates/renderer/src/vulkan/pipeline.rs#L46-L54) | ✓ holding (renderer audit PIPE-9) |
| NiMaterialProperty colors copied raw, no srgb_to_linear | [crates/nif/src/import/material.rs:786-795](crates/nif/src/import/material.rs#L786-L795) | ✓ holding (commit 0e8efc6, feedback_color_space.md) |
| z_test/z_write/z_function dynamic state per batch | [crates/renderer/src/vulkan/pipeline.rs:276-280, 478-486](crates/renderer/src/vulkan/pipeline.rs#L276-L280) | ✓ holding |
| Pre-Gamebryo v3.3.0.13 fallback log level = `debug!`, not `warn!` | [crates/nif/src/lib.rs:226](crates/nif/src/lib.rs#L226) | ✓ holding (no archive-sweep spam) |
| Legacy particle stack: import → ECS → instanced billboard renderer wired end-to-end (#401) | walk.rs / scene.rs / systems.rs / context/mod.rs | ✓ holding |
| Block dispatch coverage for every type in 04-17 H-2 list (NiFogProperty, NiColorData, NiPathInterpolator, NiFlipController, NiBSBoneLODController, NiBoolTimelineInterpolator, bhkBlendController, bhkMultiSphereShape) | [crates/nif/src/blocks/mod.rs](crates/nif/src/blocks/mod.rs) | ✓ all dispatched |

---

## Blocker Chain — what must land for "interior cell with NPCs renders"

```
1. ROADMAP/CLAUDE.md doc cleanup (O5-1)
   ↓ (independent — cosmetic/truth bug, doesn't block work)

2. Real-data parse-rate recovery (Dim 5 O5-2 + O5-3 — 95.21% → 100%)
   ↓ Bisect the 154-file root-NiNode under-consumption class via debug-mode
     consumed-byte cross-check + crates/nif/examples/trace_block.rs
   ↓ Audit crates/nif/src/blocks/particle.rs for the 84 alloc-cap files
     (NiPSysBoxEmitter / NiPSysGrowFadeModifier / NiPSysSpawnModifier)

3. KF animation chain (O6-N-01 — door idles, creature idles, NPC walks)
   ↓ Add Path-3 to import_kf for NiSequenceStreamHelper:
     - walk extra_data chain for NiTextKeyExtraData + per-bone NiKeyframeController
     - resolve target node by name via build_subtree_name_map
     - reuse extract_translation/rotation/scale_channel against NiKeyframeData
   ↓ ~1-2 days per 04-17 estimate.

4. NiStencilProperty stencil-test (#337, O4-01 — Oblivion gates render correctly)
   ↓ Drop stencil_function/ref/mask/fail_action/z_fail_action/pass_action into
     GpuInstance and pipeline create-info dynamic state.

5. (Optional) Doc-surface cleanup (O6-N-02, O6-N-03)
```

Cell-loader integration with TES4 worldspace + LAND records remains the open exterior blocker (was already on the M32+ tier list; not Oblivion-specific).

---

## Dimension Findings

### Dim 1 — NIF v20.0.0.5 Parser Correctness — `0 CRITICAL · 0 HIGH · 2 MEDIUM · 3 LOW · 3 INFO`

Parser correctness for Oblivion v20.0.0.5 remains green. Every regression guard holds; M-01..M-03 from prior audits are either confirmed not-bugs or unchanged-and-unimpactful.

**MEDIUM**:
- **[O1-1]** [NiTexturingProperty.flags zero-defaulted in v10.0.1.3..20.1.0.1 gap](crates/nif/src/blocks/properties.rs#L222-L230) — Oblivion v20.0.0.5 sits in this window, parser correctly skips read and stores 0. Risk: downstream consumer interpreting `flags == 0` as "all bits off" rather than "field absent".
- **[O1-2]** [`is_ni_node_subclass` doesn't list BSRange aliases](crates/nif/src/lib.rs#L584-L598) — fragile contract. BsRangeNode::block_type_name returns base "BSRangeNode" string today; if anyone changes that to alias-aware (`BSDebrisNode`, etc), root-picker silently skips. One-line comment fix.

**LOW**:
- **[O1-3]** M-01 (NiCamera not unwrapped) — confirmed not-a-bug. NiCamera has no children list.
- **[O1-4]** M-02 NiLODNode walker hard-picks `children[0]` — still latent, zero real-data impact on Oblivion (sparse usage).
- **[O1-5]** M-03 NiLODNode pre-10.1.0.0 path returns NULL ref — never hit on Oblivion (v20.0.0.5 ≥ 10.1.0.0).

**INFO**:
- **[O1-6]** Stale OPEN issue **#555** (NiPathInterpolator dispatch missing) — arm IS present at [mod.rs:600](crates/nif/src/blocks/mod.rs#L600) (added in #394). Close as not-applicable.
- **[O1-7]** Stale OPEN issue **#556** (NiBSBoneLODController missing) — present at [mod.rs:616](crates/nif/src/blocks/mod.rs#L616). Close as not-applicable.
- **[O1-8]** CLAUDE.md "100% on 8032 Oblivion NIFs" — see Dim 5 O5-1 below; the claim is regressed.

### Dim 2 — BSA v103 Archive — `0 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW · 0 INFO`

**Status: WORKING.** Empirical extraction sweep at 147,629 / 147,629 (100%) across all 17 vanilla Oblivion BSAs. All 12 spec-correctness invariants from the 04-17 audit re-verified at current line numbers. M-3 (file-handle reopen perf) closed by #360.

**MEDIUM**:
- **[O2-1]** [Zero on-disk v103 regression test coverage](crates/bsa/src/archive.rs#L533-L1387) — all 14 `#[ignore]`-gated integration tests target FNV v104 or Skyrim SE v105. v105 coverage filled by #569 + #617; v103 untouched. Sketch: add `#[ignore]`'d disk test against `Oblivion - Meshes.bsa` mirroring FNV's `extract_beer_bottle` pattern.

**LOW**:
- **[O2-2]** [Misleading comment "v103 uses different flag semantics for bits 7-10"](crates/bsa/src/archive.rs#L186-L187) — speculative and wrong. Real v103 semantic for bit 0x100 is "Xbox archive", not a different layout. Behavior correct; comment wrong. Lines shifted (162-164 → 186-187) post-#586 but text bit-identical.
- **[O2-3]** [Dead-code warning on `FolderRecord.{hash, offset}` in release builds](crates/bsa/src/archive.rs#L215) — fields read inside `#[cfg(debug_assertions)]` validators (#361/#362). Cosmetic.

### Dim 3 — TES4 ESM Record Coverage — `0 CRITICAL · 2 HIGH · 5 MEDIUM · 3 LOW · 4 INFO`

Walker structurally sound. All five 04-17 C/H regressions verified FIXED. Two new HIGH gaps in item-record schema; rendering-critical CELL fields covered.

**HIGH**:
- **[O3-N-01]** [Oblivion WEAP DATA collapsed onto FO3/FNV schema](crates/plugin/src/esm/records/items.rs#L147-L158) — `parse_weap` matches `Oblivion | Fallout3NV | Fallout4` together. Oblivion WEAP DATA is **15 bytes** with completely different fields (`Type/Speed/Reach/Flags/Value/Health/Weight/Damage`); FO3/FNV is 16 bytes. Every Oblivion WEAP `common.value`, `common.weight`, `Weapon.damage` is wrong. No rendering impact today; breaks future inventory/economy.
- **[O3-N-02]** [Oblivion ARMO DATA shifted by one field](crates/plugin/src/esm/records/items.rs#L250-L269) — Oblivion ARMO DATA is `armor + value + health + weight` (16 bytes); current parser groups Oblivion with FO3/FNV's 12-byte `(value, health, weight)`. Oblivion `armor` rating gets stored as `value`, real `value` lands in `health`, etc.

**MEDIUM**:
- **[O3-N-03]** [AMMO records — Oblivion has no AMMO DATA shape match](crates/plugin/src/esm/records/items.rs#L319-L339) — `clip_rounds` reads garbage from low byte of `weight`; ENAM enchantment ref dropped.
- **[O3-N-04]** [CELL parser drops XOWN / XRNK / XGLB ownership tuple](crates/plugin/src/esm/cell.rs#L743-L922) — interior + exterior. Cross-game (FO3/FNV/Skyrim use too). Stealing/property crime detection unwirable.
- **[O3-N-05]** [CELL parser drops XCMT (pre-Skyrim music type) and XCCM (climate override per cell)](crates/plugin/src/esm/cell.rs#L743-L922) — interior music selection on Oblivion/FO3/FNV uses XCMT (single-byte enum); both fall to `_ => {}`.
- **[O3-N-07]** [Top-level dispatch missing ENCH (enchantments)](crates/plugin/src/esm/records/mod.rs#L266-L457) — already tracked at FNV-D2-01 (#629). Cross-game; every enchanted item dangles.
- **[O3-N-08]** [HEDR version 1.0 used by Oblivion AND FO4 — disambiguation correct via EsmVariant, comment misleading](crates/plugin/src/esm/records/items.rs#L148-L150) — comment says "FO3/FNV WEAP DATA" without flagging Oblivion's collapse. Comment ambiguity directly enabled O3-N-01/02/03.

**LOW**:
- **[O3-N-06]** EsmIndex.total() omits seven new record categories — already tracked at FNV-D2-06.
- **[O3-N-09]** FACT u8/u32 fix (#481) didn't extend to ARMA/ARMO biped flags — but Oblivion uses BMDT, already correct. Listed for completeness.
- **[O3-N-10]** [LIGH `radius` u32-on-disk pre-cast to f32 — test fixture comment at cell.rs:2056 still says "BGRA"](crates/plugin/src/esm/cell.rs#L2054-L2056) — stale doc post-#389-revert.

**INFO**:
- **[O3-N-11]** Exterior CELL parser doesn't decode XCLL — legal per UESP, but on Oblivion essentially never authored.
- **[O3-N-12]** Walker survival at 100% on synthetic + real Oblivion — pinned by `parse_rate_oblivion_esm`.
- **[O3-N-13]** Oblivion DIAL Topic Children handled (#631 fixed).
- **[O3-N-14]** Records dispatch comprehensive at 32 record types post-#458/#519/#521/#590. Outstanding non-rendering gaps: ENCH, CSTY, LSCR, RGDL, PWAT, IDLE.

### Dim 4 — Rendering Path for Oblivion Shaders — `0 CRITICAL · 1 HIGH · 3 MEDIUM · 3 LOW · 1 INFO`

All four 04-17 C/H regressions closed. Rendering path for NiTexturingProperty + NiMaterialProperty + NiAlphaProperty + NiZBufferProperty is structurally complete and reaches the fragment shader.

**HIGH**:
- **[O4-01]** [NiStencilProperty: only `is_two_sided` consumed; stencil_function/ref/mask/fail/zfail/pass discarded](crates/nif/src/import/material.rs#L1000-L1007) — Pipeline hardcodes `stencil_test_enable(false)`. Open issue **#337**. Oblivion gates, mirrors, scrying orbs render as opaque holes.

**MEDIUM**:
- **[O4-02]** [NiVertexColorProperty.lighting_mode parsed but never consumed](crates/nif/src/blocks/properties.rs#L1330-L1366) — LIGHTING_E (vertex colors REPLACE) vs LIGHTING_E_A_D (ADD). Shader unconditionally treats vertex colors as multiplicative tint. LIGHTING_E meshes (FX) get material colors double-counted.
- **[O4-03]** [NiVertexColorProperty.vertex_mode = Emissive routes vertex color into albedo, not emissive](crates/nif/src/import/material.rs#L523-L544) — `extract_vertex_colors` falls through; flickering torches and emissive signs lose authored payload.
- **[O4-04]** [NiSpecularProperty disable leaves `specular_color` untouched](crates/nif/src/import/material.rs#L1018-L1022) — IOR glass branch silently RE-ENABLES the spec term via `specStrength = max(specStrength, 3.0)`. Affects rare meshes with explicit spec disable.

**LOW**:
- **[O4-05]** [NiWireframeProperty / NiDitherProperty / NiShadeProperty: parsed (NiFlagProperty), never read](crates/nif/src/blocks/properties.rs#L1246-L1320) — NiShadeProperty.flags & 1 == 0 forces flat shading on a handful of architectural pieces.
- **[O4-06]** [Specular gloss applied as `specStrength *= glossSample.r` rather than authored Phong-exponent multiplier](crates/renderer/shaders/triangle.frag#L728-L734) — defer behind PBR roughness pipeline cleanup.
- **[O4-07]** [Decal slots populated but emit as alpha-blend overlays rather than depth-bias decal pipeline path](crates/nif/src/import/material.rs#L863-L874) — extraction half done by #400; round-trip incomplete. Either drop extraction or finish the binding.

**INFO**:
- **[INFO O4-08]** BSEffectShaderProperty default-additive — Skyrim+ block, not Oblivion concern.

### Dim 5 — Real-Data Validation — `1 CRITICAL · 2 HIGH · 2 MEDIUM · 1 LOW · 2 INFO`

**Parse rate: 7647 / 8032 (95.21%) clean, NOT 100%**. 384 truncated, 1 hard fail. Run time 2.21s release on Ryzen 7950X. 23,645 blocks dropped across 384 files.

**Top truncation reason histogram**:
| Count | Reason class |
|-------|---|
| 154 | "failed to fill whole buffer" (root NiNode size-walk underrun) |
| 84 | "exceeds hard cap" allocation requests (corrupt count harvested via drift) |
| 68 | "unknown KeyType" on NiTransformData / NiPosData / NiFloatData |
| 18 | bogus "X-byte read at position Y, only Z remaining" (post-drift symptom) |

**Representative mesh traces**: 4 of 5 candidates passed cleanly through `import_nif_scene`. The 5th (`marker_radius.nif`) hard-failed by design (corrupt debug marker). The flagship 04-17 OOM file `upperclassdisplaycaseblue01.nif` traces to 7 nodes / 15 meshes — C-1 fix held.

**CRITICAL**:
- **[O5-1]** [ROADMAP.md:71 and CLAUDE.md claim 100% Oblivion parse rate; measured rate is 95.21%](ROADMAP.md#L71). 384 NIFs still truncate. Doc-truth bug, not behavior regression — `nif_stats` exit code is 1 (gate firing). Re-measure FNV/FO3/FO4/Skyrim/FO76/Starfield to confirm whether "100%" claim is also stale on those.

**HIGH**:
- **[O5-2]** 80+ "exceeds hard cap" allocation rejects mark `truncated=true` with whole subtrees lost — `check_alloc` correctly bounces (no abort), but truncation drops every block AFTER the failing one. Worst offenders: [crates/nif/src/blocks/particle.rs](crates/nif/src/blocks/particle.rs) (NiPSysBoxEmitter / NiPSysGrowFadeModifier / NiPSysSpawnModifier — 90+ instances combined). Files that previously OOM-aborted are now silent data loss. Root-cause fix: debug-mode per-parser consumed-byte cross-check.
- **[O5-3]** 154 files truncate at the root NiNode "failed to fill whole buffer" — root-block under-consumption. Same shape as pre-04-17 138-file class; H-1 parser additions didn't clear it. Affects `meshes\creatures\horse\horseheadgrey.nif`, `meshes\creatures\imp\imp.nif`-class siblings, and architecture pieces. Suggests a shared parent (NiAVObject? NiObjectNET?) has a field-width discrepancy on a subset of v20.0.0.5 content.

**MEDIUM**:
- **[O5-4]** `NiTransformData` shows 20 partial-unknown blocks — likely upstream stream-position drift (named block is a victim, not perpetrator). Bisect via `crates/nif/examples/recovery_trace.rs`.
- **[O5-5]** `meshes\marker_radius.nif` is the sole hard-fail; corrupt by design (debug marker). Either skip in sweep or convert alloc-cap rejection to truncation for consistency.

**LOW**:
- **[O5-6]** [imptest.rs reports `nodes=1` for many meshes that have multiple top-level nodes](crates/nif/examples/imptest.rs) — counts flat root-node count, not hierarchical depth. Tooling-precision finding.

**INFO**:
- **[O5-7]** H-1 fix verified — `upperclassdisplaycaseblue01.nif` no longer aborts. Class-wide `allocate_vec` sweep held. No process-survival regressions.
- **[O5-8]** Particle subsystem renderer integration confirmed — `byroredux/src/systems.rs:953` `particle_system` scheduled at `Stage::PostUpdate`. Visual validation of Oblivion torches still pending.

### Dim 6 — Blockers & Game-Specific Quirks — `0 CRITICAL · 1 HIGH · 2 MEDIUM · 0 LOW · 4 INFO`

**HIGH**:
- **[O6-N-01]** [KF importer still has no `NiSequenceStreamHelper + NiKeyframeController` path — every Oblivion KF produces zero clips](crates/nif/src/anim.rs#L252) — comment at [controller.rs:1835-1838](crates/nif/src/blocks/controller.rs#L1835-L1838) explicit: "remains as a follow-up". Cross-cuts FO3/FNV. Fix sketch: detect `NiSequenceStreamHelper` root → walk extra_data chain for `NiTextKeyExtraData` + per-bone `NiKeyframeController`s → reuse existing channel extractors against `NiKeyframeData` → resolve target node via existing `build_subtree_name_map`. ~1-2 days.

**MEDIUM**:
- **[O6-N-02]** [.claude/commands/audit-oblivion.md:19, 22 still asserts "decompression NOT WORKING (open blocker)"](.claude/commands/audit-oblivion.md#L19) — refuted by Dim 2. Drives every future audit on a false premise.
- **[O6-N-03]** [ROADMAP.md:63-64, 71, 102, 314 claims "BSA v103 decompression not working" in 4 places](ROADMAP.md#L63) — Strike "BSA v103 decompression" from all four sites. Replace with "Oblivion exterior blocked on TES4 worldspace + LAND wiring (same shape as FO3 was)".

**INFO**:
- **[O6-I-01]** CLAUDE.md "Oblivion → 100%" was true at 04-17 only after fix; **regressed today to 95.21%** (see O5-1).
- **[O6-I-02]** BSStreamHeader + NiTexturingProperty + user_version threshold regression guards still hold (cross-ref Regression Guard List above).
- **[O6-I-03]** `NiSequenceStreamHelper` parses cleanly — no parse-time blocker for O6-N-01.
- **[O6-I-04]** [Legacy particle preset is name-string heuristic, not data-driven](byroredux/src/scene.rs#L1018-L1042) — dust motes get flame, ghost effects get flame, water spray gets flame. Visual incorrectness, not invisibility. M36-shaped follow-up to #401.

---

## Cross-Dimension Deduplication

- **LIGH BGRA/RGB color**: 04-17 Dim 3 C2 + Dim 6 #7 → both verified RESOLVED. Original RGB read was correct; #389 was reverted post-FNV cross-check. Single stale doc-comment at [cell.rs:2056](crates/plugin/src/esm/cell.rs#L2054-L2056) (logged once as O3-N-10).
- **BSA v103 "decompression NOT WORKING"**: appears in 04-17 Dim 6 #1 AND in current `.claude/commands/audit-oblivion.md`. Logged once each in Dim 6 (O6-N-02 for slash-command file, O6-N-03 for ROADMAP).
- **CLAUDE.md "100% Oblivion"**: dim_1 (O1-8 INFO) + dim_5 (O5-1 CRITICAL) + dim_6 (O6-I-01 INFO). Promoted to CRITICAL on Dim 5 because that's where measurement happens.
- **Dim 4 O4-04 (NiSpecularProperty)** cross-references closed issue #220 — that fix is incomplete on the IOR glass branch (`triangle.frag:970` `max(specStrength, 3.0)`). Open as a follow-up.

---

## Suggested Next Steps

1. **Publish this report**: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-04-25.md`
2. **Doc-truth fixes** (10-line PR): update ROADMAP.md and CLAUDE.md to reflect 95.21% Oblivion parse rate; remove "BSA v103 decompression" from blocker lists; update slash-command file.
3. **Stale-issue cleanup**: close #555 and #556 with links to current dispatch arms.
4. **Parse-rate recovery sprint** (P0): bisect the 154 root-NiNode under-consumption files (debug-mode consumed-byte cross-check + trace_block.rs) and the 84 alloc-cap files (audit `crates/nif/src/blocks/particle.rs`).
5. **KF importer Path-3** (next renderable interior milestone): add `NiSequenceStreamHelper + NiKeyframeController` chain to `import_kf`. ~1-2 days.
6. **#337 NiStencilProperty** (Oblivion-gate visual correctness): plumb stencil_function/ref/mask + dynamic state.

---

**Audit produced**: 6 parallel dimension agents (legacy-specialist, renderer-specialist, general-purpose × 4) + orchestrator merge. Total findings: 39 across 6 dimensions. Run time ~25 min wall clock.
