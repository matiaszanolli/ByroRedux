# #3926: OBL-2026-09-05-D1-01: the sizeless runtime-size-cache recovery applies a median skip with no plausibility check, converting hard failures into silent mis-alignment

Filed from `docs/audits/AUDIT_OBLIVION_2026-09-05.md` (OBL-2026-09-05-D1-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `medium,game:oblivion,legacy-compat,nif-parser,nif,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3926 --json state`.

---

**Source**: `docs/audits/AUDIT_OBLIVION_2026-09-05.md` (OBL-2026-09-05-D1-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: 1 — NIF Version Handling (Oblivion-only code path)
- **Location**: `crates/nif/src/lib.rs`, `parse_nif`'s `Err` arm — the `parsed_size_cache` branch (`stream.set_position(start_pos)` then `stream.skip(median_size)`), reachable only when `header.block_sizes.is_empty()`.
- **Status**: NEW. Hardening of `#324` (CLOSED, "M2: Oblivion synthetic skip-table to prevent cascading parse failure").
- **Description**: When a block fails to parse on a game with no per-block size table, the recovery takes the **median of previously-observed consumed sizes for the same type in the same file** and skips that many bytes. `NiSkinPartition` is a variable-size block whose length scales with vertex/triangle/bone counts, so the median of one partition is a poor estimate of another. Nothing validates the post-skip position — no magic-number check, no "does the next block's type index look sane" test, no bound on how far the median may differ from the failed block's own partial consumption. The block is replaced with `NiUnknown`, `recovered_blocks` is bumped, and parsing continues from a position that may be arbitrarily wrong.
- **Evidence**: On HEAD, `Oblivion - Meshes.bsa` reports `recovered: 538 (2 types with partial unknown)`: `NiSkinPartition` 74 and `NiNode` **464**. With the D6-01 fix applied, both drop to **0** — so all 464 `NiNode` substitutions are *downstream cascade* from a mis-aligned stream, not independent parse failures. The mechanism turns one upstream parser bug into hundreds of `NiUnknown` blocks that the recoverable-rate gate still scores as a success. Worse than the counted case: a median skip that happens to land on a *plausible* boundary produces a block that parses "successfully" from the wrong offset and is never counted at all.
- **Impact**: Oblivion (and pre-Gamebryo NetImmerse content) only — no other shipped title reaches this branch. Defense-in-depth: any future parser bug in a variable-size Oblivion block gets its damage silently multiplied instead of stopping at one truncation, and the multiplication is invisible to every current gate. The 464:74 ratio measured today is the observable size of that multiplier.
- **Related**: `#324`; `#568` (clean-vs-recoverable split); OBL-2026-09-05-D6-01 (the bug that exercised it); PERF-D6-NEW-02.
- **Suggested Fix**: Bound the median skip — reject it when `median_size` deviates from the failed block's own `consumed` by more than some factor, or when the resulting position does not land on a boundary consistent with the next block's expected type. At minimum, promote the per-type recovery rollup from `warn!`-summary to a value on `NifScene` so a caller can distinguish "recovered cleanly" from "recovered by guessing".

---

### Existing — verified still true this run

#### #3567 — Oblivion `APPLY_HILIGHT2` normal-map alpha is consumed as both parallax height and specular mask
- **Status**: Existing: **#3567** (OPEN). Premise re-verified against current code.
- **Evidence**: `byroredux/src/render/static_meshes.rs` sets
  `PARALLAX_ALPHA_HEIGHT_BIT` when `parallax_map_index != 0 && normal_has_alpha
  && material.parallax_height_in_alpha`. The gloss-slot rebind gate,
  `normal_alpha_spec_binding_applies`
  (`byroredux/src/material_translate.rs`), takes `material: Option<&Material>`
  but consults only `env_map_scale`, `material_kind`, `normal_map_index`,
  `gloss_map_index` and `normal_has_alpha` — it **never reads
  `Material::parallax_height_in_alpha`**. An Oblivion `APPLY_HILIGHT2` material
  with a normal map, no gloss map and an alpha-bearing normal therefore sets
  both `PARALLAX_ALPHA_HEIGHT_BIT` and `NORMAL_ALPHA_SPEC_BIT` on the same
  channel. Unchanged since the issue was filed.

#### #3848 — `oblivion_ruleset` is production-unreachable
- **Status**: Existing: **#3848** (OPEN). Verified, **not** re-filed.
- **Evidence**: `CharacterRulesProfile::OBLIVION`
  (`crates/core/src/character/profile.rs`) carries `ruleset:
  RulesetBuilder::None`, and `build_ruleset`'s `RulesetBuilder::None` arm
  `return None`s before touching `oblivion_ruleset`
  (`crates/core/src/character/tes.rs`), which is otherwise complete and
  unit-tested end to end.
- **Additional Oblivion-specific consequence (for #3848, not a new issue)**:
  the same profile also carries `npc_stats: NpcStatModel::None` **and**
  `creature_stats: NpcStatModel::None`, and
  `derive_npc_actor_values`
  (`crates/plugin/src/esm/records/actor_value_derive.rs`) returns `Vec::new()`
  for that arm. So `#1650`'s Oblivion 16-byte-ACBS recovery of `level` and
  `acbs_flags`, and the `is_oblivion`-gated `ATTR` / `DNAM` / `VNAM` / `PNAM` /
  `UNAM` / `XNAM` decode in
  `crates/plugin/src/esm/records/actor/mod.rs`, currently have **no consumer**
  on the population side either — the data is parsed correctly and then
  discarded. This is deliberate and pinned by
  `oblivion_creatures_select_no_stat_model`, and is already listed in
  `docs/feature-matrix.md`, so it is a scope note rather than a defect; it
  belongs on #3848 as the second half of what "unwired" costs.

#### Oblivion ESM findings from the 2026-08-30 sweep — still open, not re-measured
`#3617` (LVSP has a `RecordType` constant but no parser, 306 leveled-spell
lists), `#3616` (only the last response of a multi-response INFO survives,
4,617 segments lost), `#3614` (TCLF / NAME / CTDT dropped on INFO). All three
require an `Oblivion.esm` parse to re-measure, which this run deliberately
skipped (see Scope caveats). No code change touching them was observed in the
static pass.

---

## Regression Guard List — verified holding

| Guard | Where | Verdict |
|---|---|---|
| `user_version` only read for `version >= V10_0_1_8` | `crates/nif/src/header.rs`, the `user_version` binding | ✅ exact |
| BSStreamHeader dual-band matches nif.xml `#BSSTREAMHEADER#` (`#170`) | `crates/nif/src/header.rs`, `has_bs_stream_header` | ✅ `V10_0_1_2` OR (`user >= 3` AND (`V20_2_0_7` \| `V20_0_0_5` \| (`V10_1_0_0..=V20_0_0_4` AND `user <= 11`))) |
| v10.x sub-version constants present as gate boundaries | `crates/nif/src/version.rs` | ✅ `V3_3_0_13`, `V4_2_2_0`, `V5_0_0_1`, `V10_0_1_2`, `V10_1_0_0`, `V10_1_0_106`, `V10_1_0_114`, `V10_2_0_0`, `V20_0_0_4`, `V20_0_0_5` all live |
| `NiGeomMorpherController` gates on `bsver > 9` (`#1509`) | `crates/nif/src/blocks/controller/morph.rs` + `MORPH_LEGACY_CUTOFF` | ✅ `MORPH_LEGACY_CUTOFF == 10`, gate is `bsver >= MORPH_LEGACY_CUTOFF` |
| v10.x stride-drift family (`#1506`/`#1507`/`#1508`) | whole-corpus outcome | ✅ 0 truncations across 9,612 NIFs once D6-01's regression is removed; 638 of the 730 HEAD truncations are D6-01, and the residual 92 are its `NiNode` cascade + recovered-only files — **no member of the v10.x family reappeared** |
| `NiTexturingProperty` reads a raw `u32` count, no `Has Shader Textures` bool | `crates/nif/src/blocks/properties.rs`, `texture_count` | ✅ raw `read_u32_le`; corpus still shows `texture_count == 7` on all 30,121 instances |
| Pre-5.0.0.1 inline block-type names log at `debug`, not per-block `warn` | `crates/nif/src/lib.rs`, the `inline_type_names` branch | ✅ one `debug!` per file; `warn!` only on a mid-file inline-name read failure |
| `NifVariant::detect` `(V20_0_0_4, uv=11)` ambiguity warning is one-shot | `crates/nif/src/version.rs`, the `std::sync::Once` block | ✅ fires once per process, not per file — no sweep spam |
| BSA v103 recognised; rejection only outside {103,104,105} | `crates/bsa/src/archive/open.rs` | ✅ |
| Folder-record size is 16 B for v103 **and** v104, 24 B only for v105 | `crates/bsa/src/archive/open.rs`, `folder_record_size` | ✅ `if version == BSA_V_SKYRIM_SE { 24 } else { 16 }` |
| v103 "Xbox archive" flag ignored for embedded names | `crates/bsa/src/archive/open.rs`, `embed_file_names` | ✅ gated `version >= BSA_V_FO3_SKYRIM` |
| BSA v103 extraction stays at 100 % (`#699`) | live sweep | ✅ 9,612 / 9,612 NIF extractions across 9 archives, **0 extract failures** |
| `havok_motion_type` maps the full nif.xml `hkMotionType` enum (`#1652`) | `crates/nif/src/import/collision/mod.rs` | ✅ 1–5/8→Dynamic, 6→Keyframed, 7→Static, 9→CharacterKinematic, 0/other→Static; pinned by `havok_motion_type_maps_full_enum` |
| `BhkMultiSphereShape` / `BhkConvexListShape` resolve, don't fall out silently | `crates/nif/src/import/collision/shape.rs` | ✅ both have downcast arms in `resolve_shape_inner` (16 arms total) |
| Oblivion 16-byte ACBS arm precedes the ≥24-byte arm (`#1650`) | `crates/plugin/src/esm/records/actor/mod.rs` | ✅ `b"ACBS" if matches!(game, GameKind::Oblivion) && sub.data.len() >= 16` sits before the Skyrim and generic ≥24 arms |
| Oblivion-specific ESM decode branches still present | `actor/mod.rs` (`is_oblivion` ATTR/DNAM/VNAM/PNAM/UNAM/XNAM/DATA), `climate.rs` (3-entry WLST), `items.rs` (Oblivion ARMO/WEAP/AMMO/CLOT arms), `cell/tests/cell.rs` (`parse_oblivion_xcll`) | ✅ present |
| `NiMaterialProperty` tags `EmissiveSource::Material` (legacy arm) | `crates/nif/src/import/material/legacy_properties.rs` | ✅ pinned by `emissive_source_tests.rs` |
| No per-draw `classify_pbr`; `Material::resolve_pbr` resolves once | `crates/core/src/ecs/components/material.rs` | ✅ `Material::classify_pbr` deleted; only the free `classify_pbr_keyword`, called from `resolve_pbr` |
| `MAT_FLAG_PBR_BSDF` unreachable across the all-legacy Oblivion universe | `crates/nif/src/import/material/mod.rs` (the `has_material_data` gate) | ✅ zero Oblivion materials author BGSM/`.mat`; Disney lobe unreachable |
| `PARALLAX_ALPHA_HEIGHT_BIT` (bit 31) masked by **both** POM marchers | `crates/renderer/shaders/include/material_sampling.glsl` (raster) and `include/ray_hit.glsl` (secondary ray) | ✅ both `& ~PARALLAX_ALPHA_HEIGHT_BIT` before indexing, both read the flag separately; `triangle.frag` masks at both of its test sites |
| `normal_has_alpha` gate on the parallax bit (`#3562`) | `byroredux/src/render/static_meshes.rs` | ✅ bit set only when the bound normal texture's DDS format actually carries alpha |
| Typed particle emitter blocks parse on Oblivion (`#1239`) | `crates/nif/src/blocks/particle.rs` + corpus | ✅ `NiPSysEmitter` 547 / 0 unknown, `NiPSysEmitterCtlr` 547 / 0, `NiPSysGrowFadeModifier` 449 / 0, `NiParticleSystem` 547 / 0, `NiPSysBlock` 4,232 / 0 in the fixed build; `extract_emitter_params` / `extract_emitter_rate` → `apply_emitter_params` chain intact |
| `_far.nif` placement LOD is Oblivion-only | `byroredux/src/cell_loader/placement_lod.rs`, `placement_lod_supported` | ✅ `game == GameKind::Oblivion`, pinned by `placement_lod_supported_is_oblivion_only`; the vanilla data backs it — `Oblivion - Meshes.bsa` carries **130** `_far.nif` and **9,944** `distantlod\` entries, and `archive_path_matches_vanilla_filenames` asserts the exact vanilla naming |

---

## Blocker Chain

Interiors already render end-to-end (Anvil Heinrich Oaken Halls) and exterior
cells already render on-device (Tamriel `(0,0)` radius 1, 6,043 entities /
2,355 draws, 2026-08-12 EX-01/EX-05). The chain to *first render* is closed —
do not regenerate the stale "BSA v103 broken" or "TES4 worldspace wiring
missing" framings.

The live chain today is:

1. **Restore the 730 lost meshes** — OBL-2026-09-05-D6-01. Until this lands,
   any readiness matrix run on Oblivion is measuring a corpus with 7.6 % of its
   meshes truncated, and 24 % of its ragdoll constraints missing. **This now
   gates the readiness matrix, not the other way round.**
2. **Re-baseline and re-gate** — regenerate nothing (the checked-in baselines
   are already correct and are what proves the regression); instead add the
   *clean*-rate floor so the next slide is caught automatically.
3. **Repeatable readiness matrix** (`#2377` / `#2368`) — the pre-existing
   remaining chain, now unblocked by (1).
4. Any placement / LOD gaps that matrix surfaces.

---

## Verified-Clean Areas (no finding)

- **Dimension 2 — BSA v103.** Regression guard holds in full. All nine
  archives opened; 9,612 of 9,612 NIF entries extracted with zero failures.
  Version acceptance, folder-record sizing, archive-flag semantics and the
  hash function are all unchanged and correct.
- **Dimension 1 — version handling.** Every guard in the checklist verified
  (table above). No new version-gate drift found. The one live defect on this
  dimension is D1-01's recovery-path hardening gap, filed above.
- **Dimension 4/5 — render + NIFAL.** No new finding. The `APPLY_HILIGHT2`
  route reaches the GPU as designed post-`#3596`/`#3562`, both marchers mask
  the flag bit, `MAT_FLAG_PBR_BSDF` is unreachable for Oblivion, PBR resolves
  exactly once, and the legacy `NiMaterialProperty` emissive arm is intact.
  The single open defect is `#3567`, re-verified rather than re-filed.
- **Dimension 7 — exterior + quirks.** `placement_lod_supported` is
  Oblivion-only and backed by real vanilla data; the pre-Gamebryo inline-name
  path logs at `debug`; the `NifVariant` ambiguity warn is one-shot. The
  `#1219` `(V20_0_0_4, uv=11)` ambiguity remains harmless
  (`havok_scale_for` maps Oblivion and Fallout3 to the same 7.0 scale) —
  unchanged, no action.
- **Dimension 3 — ESM.** Static verification only (see Scope caveats). All
  Oblivion-specific decode branches named in the checklist are present and
  structurally correct. Real-data parity was **not** re-run; #3614/#3616/#3617
  remain open and unmeasured this cycle.

---

## Documentation drift observed (fold into the fix, don't file separately)

- `ROADMAP.md:605` records Oblivion at "**100%** (8 032 / 8 032) · recover
  100%". Live measurement is 7,454 / 8,032 (92.80 %) for that archive and
  8,882 / 9,612 (92.41 %) across the full corpus. The row becomes correct again
  the moment D6-01 is fixed — it should be re-verified, not rewritten.
- `crates/nif/examples/nif_stats.rs`'s `--tsv` histogram still keys parsed
  blocks on the parsed struct's `block_type_name()`, while
  `crates/nif/tests/per_block_baselines.rs` keys on wire RTTI since `#3326`.
  The two therefore emit different type-name sets for the same corpus
  (`NiPSysBlock` / `NiExtraData` collapses vs. per-wire-type rows). Already
  reported by `/audit-nif` as D3-03; noted here only because it makes an
  auditor's first instinct — diff the tool's TSV against the checked-in
  baseline — misleading on every row except the non-aliased ones.

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_OBLIVION_2026-09-05.md
```

Label OBL-2026-09-05-D6-01 `critical` · `bug` · `nif-parser` · `nif` ·
`legacy-compat` · `game:oblivion` (and cross-link the FO3/NIF root-cause
issue rather than opening a second one for the same line of code).
Label OBL-2026-09-05-D1-01 `medium` · `bug` · `nif-parser` · `nif` ·
`legacy-compat` · `game:oblivion`.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
