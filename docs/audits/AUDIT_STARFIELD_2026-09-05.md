# Starfield Compatibility Audit — 2026-09-05

**Command**: `/audit-starfield` (full, all 9 dimensions), run as part of the
`texture-roles-deep` preset.
**HEAD**: `fa5c4191`
**Game data**: present — `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` (129 archives)
**Dedup baseline**: `gh issue list` — 100 open (`/tmp/audit/issues.json`) + 1,000 all-state
(`/tmp/audit/issues_all.json`), plus a scan of `docs/audits/`.
**Preset focus**: the 2026-07-27 cross-game texture-role unification (`1d94eb24`, `05d68926`,
`c8c8a834`) — `MaterialTextureSet`, `ImportedMaterial`, `merge_external_material`,
`translate_material` — with extra weight on the `materialsbeta.cdb` path.

**Findings: 6 — CRITICAL 0 · HIGH 1 · MEDIUM 1 · LOW 4.**

---

## Executive Summary

Starfield's bring-up surface is in good shape. Every named regression guard this skill lists
was re-read against current code and every one is intact: BA2 v3's 36-byte header and
`compression_method` dispatch (hard-erroring on an unsupported method, not falling through),
the `packed_size == 0` per-chunk raw/LZ4 selector on both GNRL and DX10, `normalize_mesh_path`'s
`geometries\` passthrough (#1292), the sentinel-slot skip on both BSGeometry stages
(#1828/#1829), the optional meshlet trailer gate (#3777), the decline-on-ambiguity bone-name
solver (#3549), the `read_starfield_tail` capture-to-`block_size` (#1606), the PDCL named skip
(#1568), `XCLL_SIZES_STARFIELD` (#1291), the `base_layer` collision gate (#1294), and the
scan-don't-hardcode CDB discovery (#1571). No regression of any of them was found.

**On the preset's focal question — how CDB texture slots map into the canonical role
vocabulary — the answer is that they do not map at all yet, and that is the honest state
rather than a defect to file.** `discover_starfield_cdbs` runs `probe_header` and hands the
result to `register_starfield_cdb_probe(_info: CdbHeaderInfo)`, which discards it and
increments a counter; `has_starfield_cdb()` is a `> 0` test; the `.mat` arm of
`merge_external_material` flips one routing bit and returns `PresenceOnly`. That is #3398's
scope (its Phase-2 blocker is the ~9.19 GB-per-CDB parse peak, and the lookup key and field
vocabulary are *solved*, not unknown). This audit did not re-report it, did not parse the CDB,
and did not launch an engine. What it *did* find in that neighbourhood is one downstream
consequence #3398 does not cover (the `Mat` provenance label is structurally unreachable, so
`mat.dump` can never print `src=mat`) and one latent role-vocabulary inconsistency that will
bite the moment Starfield content *does* author a `BSShaderTextureSet`.

The one serious finding is not in the material layer at all: **commit `3562401b` (today,
#3637) inverted archive lookup from first-listed-wins to last-listed-wins, and the shipped
Starfield launch profile still orders its archives on the old premise.** Under the new
precedence, `Starfield - MeshesPatch.ba2`, `LODMeshesPatch.ba2` and `TexturesPatch01/02.ba2`
are shadowed by the base archives listed after them — every patched Starfield mesh and texture
silently reverts to its unpatched version on the default `--game starfield` launch. The FNV
profile has the identical inversion, and a green test actively pins it in place.

### Coverage / method limits

Two dimensions were **not fully exercised**, deliberately and by instruction:

- **Dim 4 (ESM resolve rate)** — `--sf-smoke` was not run. Parsing the 1.36 GB `Starfield.esm`
  in-process is the known 20+ GB memory-spike class (`plugin_ignored_tests_oom`) and the brief
  forbids launching an engine. Guards were checked statically; the resolve-rate number is **not
  re-measured** this pass.
- **Dim 7 (real-data validation)** — `parse_rate_starfield_all_meshes` was not run for the same
  reason. Parse rates are **not re-measured**; the corpus enumeration was verified statically.

Neither dimension's *guards* showed drift, but neither number in ROADMAP was re-confirmed.
Anyone relying on this report for a parse-rate delta should run those two gates.

---

## Findings

### SF-2026-09-05-D7-01: the Starfield launch profile lists its patch archives first, but #3637 inverted archive precedence to last-listed-wins today — MeshesPatch, LODMeshesPatch and both TexturesPatch archives are now fully shadowed

- **Severity**: HIGH
- **Dimension**: 7 — Real-data validation / archive feed (cross-cuts Dim 1 and Dim 2)
- **Location**: `assets/debug_profiles.toml:183-209` (Starfield) and `:47-56` (FNV);
  precedence site `byroredux/src/asset_provider/texture.rs:36-42`, `:65-70`, `:87-92`, `:133-138`;
  ordering pinned by `crates/game-detect/src/lib.rs:163-204`
- **Status**: NEW (no matching issue in the 1,000-issue all-state listing; #3637 is the *cause*
  and is CLOSED as of today 14:38, #3790 is the FNV sibling and is CLOSED)
- **Description**: `3562401b` ("Fix #3637: mesh/texture/material archive lookup is last-wins,
  not first-wins", 2026-09-05 11:37) changed `TextureProvider::extract` / `extract_mesh` /
  `extract_mesh_exact` and `MaterialProvider::extract_from_archives` to walk their archive
  chains with `.iter().rev()`. The shipped launch profiles were not updated. The Starfield
  profile still carries the pre-fix rationale in a comment — *"MeshesPatch must precede the base
  archives because archive lookup is first-listed-wins"* — and orders its archives accordingly.
  `boot.rs:2288-2305` pushes `default_bsas` / `default_textures_bsas` into `--bsa` /
  `--textures-bsa` in profile order, and `open_with_numeric_siblings`
  (`asset_provider/archive.rs:459-483`) pushes into the provider in that same order. So the
  effective priority is now the exact reverse of what the profile intends.
- **Evidence**:
  ```toml
  # assets/debug_profiles.toml:183-193
  # R5.5 material-provider matrix baseline. MeshesPatch must precede the base
  # archives because archive lookup is first-listed-wins. LOD/face archives are
  # included for the same complete Cydonia path documented by game-compatibility.
  default_bsas = [
      "Starfield - MeshesPatch.ba2",     # now LOWEST priority
      "Starfield - Meshes01.ba2",
      "Starfield - Meshes02.ba2",
      "Starfield - LODMeshesPatch.ba2",  # now loses to LODMeshes below it
      "Starfield - LODMeshes.ba2",
      "Starfield - FaceMeshes.ba2",      # now HIGHEST priority
  ]
  ```
  ```rust
  // byroredux/src/asset_provider/texture.rs:87-92
  // #3637 — last-listed archive wins; see `extract`'s doc for why.
  for archive in self.mesh_archives.iter().rev() {
      if let Ok(data) = archive.extract_mesh(...) { return Some(data); }
  }
  ```
  The same inversion applies to `default_textures_bsas:194-209`: `TexturesPatch01/02` are
  listed first and are now beaten by `Textures01`…`Textures11`.
  The FNV profile is the same bug with an explicit guard test *enforcing* the wrong order:
  ```rust
  // crates/game-detect/src/lib.rs:199-202
  assert!(update_pos < base_pos,
      "Update.bsa must precede Fallout - Meshes.bsa in fnv.default_bsas \
       {default_bsas:?} — archive resolution is first-listed-wins, so this \
       order is what makes the patch archive actually win");
  ```
  That test is **green and wrong**: its stated premise ("archive resolution is
  first-listed-wins") was falsified six hours before this audit, so the regression is pinned in
  place rather than caught. By contrast the FO4 profile happens to be *correct* after #3637 —
  `Fallout4 - TexturesPatch.ba2` is listed **last** (`:150-162`) — which is precisely the
  inconsistency that shows the ordering was never a single coherent convention.
- **Impact**: On the default `--game starfield` launch, every asset that Bethesda's own patch
  archives override reverts to its unpatched base version — meshes, LOD meshes and textures
  alike. This is silent: `count_shadowed_entries` (#3637's own diagnostic) logs a shadow count
  at boot, but nothing decides *which* side should win, and the visible symptom is
  indistinguishable from "the base asset is what shipped". Blast radius is the whole Cydonia
  path this profile exists to drive, plus the FNV `Update.bsa` override set (21 base-game MODL
  paths incl. the NCR guard towers, per #3790) on `--game fnv`.
- **Related**: #3637 (CLOSED, the precedence flip), #3790 (CLOSED, the FNV ordering + its now
  wrong-premise test), #1776 / #2584 / #2621 (the sibling auto-load machinery that also pushes
  in order).
- **Suggested Fix**: Reverse the patch-archive position in the `starfield` and `fnv` profiles
  (patch archives last), rewrite both ordering comments to state last-listed-wins, and invert
  the assertion in `fnv_profile_lists_update_bsa_before_the_base_meshes_archive` along with its
  doc comment. Consider adding the Starfield sibling assertion so both profiles are pinned, and
  a single shared doc line naming the convention once so the next flip has one place to update.

---

### SF-2026-09-05-D8-01: `slot_to_role` puts Starfield on Skyrim's slot vocabulary while `canonical_shader_type` puts it on FO76's — one file, two answers, neither backed by Starfield evidence

- **Severity**: MEDIUM
- **Dimension**: 8 — NIFAL canonical material translation
- **Location**: `crates/nif/src/import/material/slot_role.rs:167-181` (`canonical_shader_type`)
  vs `:288-428` (`slot_to_role`), specifically the slot 2 / 3 / 6 / 7 arms at `:306`, `:343`,
  `:398-403`, `:412-426`
- **Status**: NEW — distinct from #3796 (CLOSED), which settled a *documentation* contradiction
  ("are the nine Starfield arms dead code or a shipped fix?") by censusing zero occupancy. This
  finding is about *which layout the arms should be grouped with*, a question #3796 neither
  asked nor answered; its own SIBLING checkbox ("the arms are labelled consistently") is what
  surfaced it.
- **Description**: `canonical_shader_type` translates Starfield's shader-type integer using
  FO76's rules, and its doc states the reason explicitly and correctly: which enum an integer
  came from *"is decided by the parser boundary, not by the slot-table layout tag"* —
  `BSLightingShaderProperty::parse_with_size` routes everything at `bsver >= FO76 (155)` through
  `parse_fo76_plus`, and Starfield is 172+. The `BSShaderTextureSet` bound to that property
  comes off the same parser boundary. Yet `slot_to_role` groups
  `TextureSlotLayout::Starfield` with **`Skyrim`** on every polymorphic slot, and the FO76
  readings it thereby rejects are the measured ones:

  | slot | Skyrim arm (what Starfield gets) | FO76 arm (what the shared parse path implies) |
  |---|---|---|
  | 2 | tint / glow / lighting-mask multiplex | tint, else glow only when `glow_map` |
  | 3 | `FaceTint → Detail`, else **`Height`** (POM) | **`GreyscaleLut`** |
  | 6 | `MultiLayerParallax → InnerLayer`, else `None` | **`Specular`** (1,616 of 1,664 `_s.dds`, #3085) |
  | 7 | back-light / model-space specular | `None` |

  Every occupancy figure cited by the Skyrim, FO4 and FO76 arms is measured on those games'
  corpora. **No arm cites a Starfield measurement**, and the module header's own census
  (2026-08-31, 60,816 NIFs, 2,564 non-stub `BSLightingShaderProperty` at bsver ≥ 172) found
  zero populated Starfield texture sets — so the grouping was never testable against content.
  The result is a mixed vocabulary: a Starfield property's *shader type* is decoded as FO76,
  and its *slots* are then read as Skyrim.
- **Evidence**:
  ```rust
  // slot_role.rs:167-172 — shader type: Starfield groups with FO76
  pub const fn canonical_shader_type(layout: TextureSlotLayout, raw: u32) -> u32 {
      if matches!(layout,
          TextureSlotLayout::Fallout76 | TextureSlotLayout::Starfield) { … }
  ```
  ```rust
  // slot_role.rs:343-352 — slot 3: Starfield groups with Skyrim, i.e. POM height
  (TextureSlotLayout::Skyrim | TextureSlotLayout::Starfield, 3) => match shader_type {
      bs_lighting::FACE_TINT => Some(TextureRole::Detail),
      _ => Some(TextureRole::Height),
  },
  (TextureSlotLayout::Fallout4 | TextureSlotLayout::Fallout76, 3) => {
      Some(TextureRole::GreyscaleLut)
  }
  ```
  ```rust
  // slot_role.rs:398-408 — slot 6: Starfield with Skyrim (usually None); FO76 = Specular
  (TextureSlotLayout::Skyrim | TextureSlotLayout::Fallout4 | TextureSlotLayout::Starfield, 6)
      => match shader_type { bs_lighting::MULTI_LAYER_PARALLAX => Some(InnerLayer), _ => None },
  (TextureSlotLayout::Fallout76, 6) => Some(TextureRole::Specular),
  ```
- **Impact**: Zero today — the census is zero, so these arms are inert and no shipped Starfield
  surface is mis-shaded. The exposure is forward: the first Creation/mod asset (or a future
  vanilla update) that authors a `BSShaderTextureSet` at bsver ≥ 172 binds slot 3 into the POM
  `Height` role — the exact `triangle.frag` failure #2694 fixed for Skyrim FaceTint heads, whose
  POM branch gates only on `parallaxMapIndex != 0u` — and drops slot 6 specular on the floor.
  Because `record_unrouted_texture_slot` counters are runtime-only and nothing consumes them in
  CI, a wrong-role bind (as opposed to an unrouted one) produces no signal at all. This is
  precisely the "invisible in one game, wrong in another" class the boundary exists to prevent,
  and it reaches the canonical `Material` at the single NIFAL boundary with no per-draw
  fallback to mask it — hence MEDIUM rather than LOW despite the zero live occupancy.
- **Related**: #3796 (CLOSED, the doc contradiction + the census), #3364 (CLOSED, the
  shader-type translation this contradicts), #3085 (CLOSED, FO76 slot 6 = specular),
  #2997 (CLOSED, FO4/FO76 slot 3 = greyscale LUT), #2694.
- **Suggested Fix**: Decide the grouping on the parser-boundary argument
  `canonical_shader_type` already makes — i.e. move `Starfield` onto the FO76 arms for slots
  2/3/6/7 — or, if the Skyrim grouping is deliberate, say why in each arm the way every other
  arm cites its evidence. Either way the two functions should stop giving opposite answers.
  Pin whichever choice is made with a `TextureSlotLayout::Starfield` fixture test (the file's
  `roles_are_unique_per_shader_type` already enumerates the layout, so the harness exists).

---

### SF-2026-09-05-D9-01: `record_external_texture_sources` re-lists all 22 roles by hand with no exhaustiveness guard, the one role-walk in the set that has none

- **Severity**: LOW
- **Dimension**: 9 — BGSM/BGEM external material flow
- **Location**: `byroredux/src/asset_provider/material.rs:1053-1090`
- **Status**: NEW
- **Description**: `MaterialTextureSet` has three generic walks (`roles()`, `values()`,
  `map_ref`) and two guard tests that cross-check the first two against `map_ref`'s
  construction-forced field coverage — `roles_covers_every_field_in_the_set` (#3349,
  `crates/nif/src/import/types.rs:1849-1863`) and `values_covers_every_field_in_the_set`
  (#3734, `:568-582`). Both exist because a hand-written role list silently stops reporting a
  newly added role. `record_external_texture_sources` is a fourth such walk — a `macro_rules!`
  invoked once per role, plus a decal loop — and it has neither a guard nor a reason to be
  hand-written: `zip_map_ref` (`types.rs:438-472`) already expresses exactly this
  before/after comparison and touches every field by construction.
- **Evidence**:
  ```rust
  // material.rs:1058-1085 — 22 hand-written invocations, unguarded
  macro_rules! record {
      ($field:ident) => {
          if before.$field.is_none() && material.textures.$field.is_some() {
              material.texture_sources.$field = source;
          }
      };
  }
  record!(base_color);
  record!(normal);
  …
  record!(glass_dirt_overlay);
  ```
  Grepping for a guard finds only four field-level assertions in
  `byroredux/src/asset_provider/tests/bgsm_merge.rs` (`base_color`, `normal`, `emissive`) —
  no coverage test.
- **Impact**: Correct today; all 22 named roles plus the decal loop are present, verified
  field-by-field against the struct. The exposure is the next role added: it gets a path from
  BGSM/BGEM but keeps `ImportedTextureSource::NifTextureSet`, so `mat.dump` reports the wrong
  provenance for it and the `tex.missing`/`mat.dump` correctness oracle quietly stops being an
  oracle for that role. Same failure mode #3349 and #3734 were each filed for.
- **Related**: #3349, #3734 (both CLOSED — the two guards this site lacks), #3814 (CLOSED — the
  equivalent source-scan guard on the renderer side).
- **Suggested Fix**: Replace the macro with `material.texture_sources = before.zip_map_ref(...)`
  over `(before, after, existing_source)`, or add the same `map_ref`-count cross-check the other
  two walks carry.

---

### SF-2026-09-05-D3-01: the `Mat` texture provenance is structurally unreachable — `mat.dump` advertises a `src=mat` label no code path can produce

- **Severity**: LOW
- **Dimension**: 3 — CDB material database correctness
- **Location**: `byroredux/src/asset_provider/material.rs:2031-2037`;
  `crates/nif/src/import/types.rs:305-311` (`ImportedTextureSource::Mat` at `:310`);
  `byroredux/src/components.rs:337-380` (`MaterialTextureSource::Mat` at `:356` + its `"mat"` label at `:372`)
- **Status**: NEW — a downstream consequence of #3398's Phase-1 state, not covered by it
  (#3398 is scoped to extracting the fields; nothing in it mentions the provenance vocabulary)
- **Description**: `merge_external_material`'s tail assigns
  `ImportedTextureSource::Mat` on `dispatch_kind == None`. That arm is unreachable: the
  function's body is `if dispatch_kind == Some(Bgsm) {…} else if dispatch_kind == Some(Bgem)
  {…} else { …; return }`, so control only reaches the tail when `dispatch_kind` is `Some`.
  `ImportedTextureSource::Mat` has no other producer in the workspace, so
  `MaterialTextureSource::Mat` (its `From` image) and the `"mat"` label it prints are dead
  ends. This is a true statement about the current design rather than a bug — the `.mat` arm
  short-circuits into `apply_cdb_pbr_fallback` and forwards no texture at all, so there is
  nothing to attribute — but the diagnostic vocabulary does not say so.
- **Evidence**:
  ```rust
  // material.rs:2031-2037
  let source = match dispatch_kind {
      Some(MaterialKind::Bgsm) => ImportedTextureSource::Bgsm,
      Some(MaterialKind::Bgem) => ImportedTextureSource::Bgem,
      None => ImportedTextureSource::Mat,   // unreachable: the `else` arm above returns
  };
  record_external_texture_sources(material, &textures_before, source);
  ```
  ```rust
  // material.rs:970-992 — the .mat arm forwards nothing to attribute
  fn apply_cdb_pbr_fallback(material: &mut ImportedMaterial, path: &str) -> MergeOutcome {
      material.is_pbr = true;
      …
      MergeOutcome::PresenceOnly
  }
  ```
  `grep -rn "ImportedTextureSource::Mat"` across the workspace returns exactly the two
  definition sites and the one unreachable assignment.
- **Impact**: Diagnostics-integrity only, but it is the diagnostic an operator would reach for
  first when asking "did this Starfield surface get any authored material data?". `mat.dump`
  offers a `mat` provenance that can never print, so a reader who sees `src=nif-texture-set`
  on every Starfield role cannot tell "the CDB supplied nothing" from "the CDB path is not
  wired". Given that CDB is Starfield's *only* real material source (zero `.bgsm`/`.bgem` files
  exist in any vanilla archive), that is the whole population.
- **Related**: #3398 (OPEN, CDB Phase 2 — the arm that would make this reachable), #2709
  (CLOSED, `MergeOutcome::PresenceOnly`, which solved the sibling "resolved to nothing" signal
  gap at the merge return but not at the per-role provenance).
- **Suggested Fix**: Either mark the arm with an explicit `#[allow]`/`unreachable!` plus a
  comment naming #3398 as what would make it live (matching the `#3230 SIBLING` comment 15
  lines above, which does exactly this for its own unreachable case), or leave the enum variant
  and have `mat.dump` state that no `.mat` provenance is currently producible.

---

### SF-2026-09-05-D8-02: `docs/engine/nifal.md` states the two BGEM glass roles are not routed into `MaterialTextureSet`, contradicting its own role count 33 lines earlier and the shipped code

- **Severity**: LOW
- **Dimension**: 8 — NIFAL canonical material translation (doc-rot)
- **Location**: `docs/engine/nifal.md:522` vs `docs/engine/nifal.md:489`
- **Status**: NEW
- **Description**: The NIFAL spec's "parked / dropped inventory" table still carries a row
  claiming `glass_roughness_scratch` / `glass_dirt_overlay` are *"decoded correctly … but **not**
  routed into `MaterialTextureSet<T>` — no 19th/20th named role exists for them"*, with
  `byroredux/src/asset_provider/material.rs` cited as *"carr[ying] an honest deferred comment at
  the short-circuit site, #2109"*. All three claims are false at HEAD: both roles are named
  fields of `MaterialTextureSet` (`crates/nif/src/import/types.rs:349-352`), filled by the BGEM
  arm of `merge_external_material` (`material.rs:1940-1952`), and forwarded to
  `supplemental_texture_slot::GLASS_ROUGHNESS_SCRATCH` / `GLASS_DIRT_OVERLAY`
  (`byroredux/src/render/static_meshes.rs:722-725`). #2109 is CLOSED, and no `#2109` comment
  survives anywhere in the source tree — the string appears only in this doc line and in prior
  audit reports. The same document says *"Its 22 named roles plus four ordered decal layers"* at
  `:489`, a count that only works if these two roles are counted, so the file contradicts itself.
- **Evidence**:
  ```
  docs/engine/nifal.md:489: `MaterialTextureSet<T>`. Its 22 named roles plus four ordered decal layers cover
  docs/engine/nifal.md:522: | `glass_roughness_scratch` / `glass_dirt_overlay` | `BGEM` … | **not** routed into `MaterialTextureSet<T>` — no 19th/20th named role exists …
  ```
  ```console
  $ grep -rn "2109" --include="*.rs" byroredux crates
  (no matches)
  ```
- **Impact**: `docs/engine/nifal.md` is the authority for the canonical role vocabulary — it is
  what an implementer or auditor reads before touching `translate_material`. A stale
  "deferred / not routed" row invites re-implementing a wired path, or (worse) reading the
  parked-inventory table as trustworthy elsewhere in the same table. This is the doc-rot class
  the project's own audit-hygiene rule exists for: a stale fact becomes a false premise a later
  auditor checks and "corrects" in the wrong direction.
- **Related**: #2109 (CLOSED — the fix this row predates), `1d94eb24` (the role unification),
  #3796 (the same class of internal self-contradiction inside `slot_role.rs`).
- **Suggested Fix**: Delete the row from the parked/dropped table; if a residue is still open
  (the four glass scalars' *shader* consumption, as opposed to the texture roles), state that
  narrower fact instead.

---

### SF-2026-09-05-D8-03: `_audit-common.md` documents `MaterialTextureSet` as having 18 named roles; the live struct has 22

- **Severity**: LOW
- **Dimension**: 8 — NIFAL canonical material translation (doc-rot, audit infrastructure)
- **Location**: `.claude/commands/_audit-common.md:97`
- **Status**: NEW
- **Description**: The shared audit protocol's POST-REFACTOR SHAPE block, which every audit
  skill reads as its layout authority, describes `MaterialTextureSet<T>` as replacing per-game
  slot numbers with *"18 named source-agnostic roles + `decals: [T; 4]`"*. The live struct
  (`crates/nif/src/import/types.rs:325-357`) declares 22 named roles plus the four decals: the
  block predates `lighting_mask` / `back_lighting` (#3458 / #2742) and
  `glass_roughness_scratch` / `glass_dirt_overlay` (#2109). `docs/engine/nifal.md:489` already
  says 22, so the two authorities disagree.
- **Evidence**: `MaterialTextureSet` fields, in declaration order — base_color, normal,
  emissive, detail, smooth_spec, dark, height, environment, environment_mask, tint, inner_layer,
  specular, lighting_mask, back_lighting, lighting, flow, wrinkle, greyscale_lut, reflectance,
  emittance_gradient, glass_roughness_scratch, glass_dirt_overlay = **22**, `+ decals: [T; 4]`.
  Confirmed independently by `roles()` (`types.rs:369-403`) enumerating 26 entries.
- **Impact**: Audit-reference integrity. `_audit-common.md` is the file every audit skill defers
  to for "what shape is this layer"; the count is the kind of number the project's own
  path/symbol gate was built to keep honest (its rationale cites `GpuMaterial` still being
  documented at 300 B after it grew to 348 B). An auditor who trusts 18 will not notice four
  unaudited roles.
- **Related**: SF-2026-09-05-D8-02 (the sibling drift in `nifal.md`), #1114 (the path-reference
  convention and its validate gate), #3439 (OPEN — the gate's known blind spots).
- **Suggested Fix**: Change 18 → 22 in `.claude/commands/_audit-common.md:97`, and consider
  phrasing it as "the roles `MaterialTextureSet::roles()` enumerates" so the number stops
  needing manual maintenance.

---

## CRC32 Flag Table

Requested by the skill's Phase 3. **No CRC32 hash → flag-name table exists anywhere in the
tree**, and none is derivable from this pass — the hashes are stored and compared as opaque
`u32`s.

- Storage: `sf1_crcs` / `sf2_crcs` on `BSLightingShaderProperty`
  (`crates/nif/src/blocks/shader.rs:707-709`) and `BSEffectShaderProperty` (`:527-528`),
  parsed by `parse_skyrim_shader_base` (`:417-447`) for `bsver >= FO4_CRC_FLAGS` (132), with the
  SF2 array gated on `bsver >= FO76_SF2_CRCS` (152).
- Consumption: exactly **four** semantic predicates read the arrays, via typed-word ∪ CRC-list
  union helpers, so that FO76/Starfield content (which writes the typed words as zero) still
  surfaces them — `is_soft_effect_from_modern_shader_flags`,
  `is_palette_color_from_modern_shader_flags`, `is_palette_alpha_from_modern_shader_flags`,
  `is_effect_lit_from_modern_shader_flags`
  (`crates/nif/src/import/material/shader_data.rs:44-62`).
- Everything else in the SLSF1/SLSF2 vocabulary is invisible on BSVER ≥ 132 content. Building
  the table would mean CRC32-ing the known nif.xml flag-name strings and matching against
  observed hashes — mechanically cheap, but it requires a corpus run this audit deliberately did
  not perform, and **guessing the hash parameterisation is exactly what the project's
  no-guessing rule forbids**. Note that the CDB spike (#3398) already established Bethesda's
  string hash as reflected CRC-32 (poly `0xEDB88320`, init 0, no final XOR) over lowercased
  paths — a strong starting hypothesis for the shader-flag names, but *unverified for this
  vocabulary*. Recorded as a known gap, not filed.

---

## Remaining-Work Chain

Per `docs/engine/starfield-esm-roadmap.md` — Phases 0 and 1 are done and Phases 2-4 were
invalidated by the 99.9%-parity measurement. Neither "BGSM parser first" nor "ESM very far" is
the shape of what is left; both have shipped. In order:

1. **Per-field CDB extraction (#3398 / #1289 Phase 2).** `.mat`-resolved materials currently
   reach the Disney lobe with NIF defaults. The lookup key and the field vocabulary are
   **solved** as of 2026-08-29 (`docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md`): the key is
   `BSResource::ID` (directory and stem hashed separately, reflected CRC-32, 98.3% match), and
   ~20 relevant `BSMaterial::*` classes are already tabulated against their `ImportedMaterial`
   targets. What remains is (a) an **indexed reader** that avoids the corpus-wide peak — 13
   CDBs / 3,077,172 chunks / ~232 MB, **two** of them full-size, so a Phase-2 reader reusing
   today's `parse` would peak near ~18 GB, not the single-CDB 9.19 GB figure — and (b) the
   *XMCOLOR* field-offset fix in `read_user_class` (verified still present this pass).
2. **PDCL ahead of GBFM** (SF-D4-01). The baseline doc's own promote/defer rule fires *defer*
   for GBFM at 0.081% of unresolved Cydonia REFRs, while PDCL sits unranked at 74.9% — roughly
   900× more impactful by the same metric.
3. **Exterior worldspace tiles.**
4. **Space-cell / planet / GBFM records.**
5. **The #2105/#3524 NIF truncation tail** (`BSWeakReferenceNode` residual).

Ahead of all of these, SF-2026-09-05-D7-01 should be fixed: it is a one-line reorder in a data
file and it currently makes the default Starfield launch render unpatched content.

---

## Dimension Summary

| # | Dimension | Result |
|---|---|---|
| 1 | BA2 v2/v3 — LZ4 block decompression | PASS — every checklist item green (hard error on unknown method, `packed_size == 0` selector on both GNRL and DX10, `max_size` supplied and bounds-checked). Existing: #3659 (mutex held across inflate). |
| 2 | BSGeometry mesh extraction | PASS — #1209/#1292/#1828/#1829/#3549/#3777/#1232 all intact. |
| 3 | CDB material database | PASS on discovery/parse discipline (#1571/#762/#2633/#2102/#2100). CDB→role mapping does not exist (=#3398, scoped not re-filed). 1 new LOW. |
| 4 | Starfield ESM resolve-rate baseline | **NOT FULLY EXERCISED** — guards static-verified, number not re-measured. Existing: #2637, #1576. |
| 5 | ESM + cell bring-up regression surface | PASS — #1567/#1568/#1291/#1294/#1235/#1295/#1284 intact; collider ghosts still structurally BLAS-excluded. |
| 6 | NIF shader blocks BSVER 155+ | PASS — #1510/#1606/#3396 intact, both shader properties get `block_size`. Existing: #2625. |
| 7 | Real-data validation | **NOT FULLY EXERCISED** for parse rate. 1 new HIGH found in the archive feed. |
| 8 | NIFAL canonical translation | PASS on the boundary itself (single boundary, plain-`f32` resolve-once, all 26 roles reach a GPU sink). 1 new MEDIUM + 2 new LOW. |
| 9 | BGSM/BGEM external material flow | PASS — narrow `&mut ImportedMaterial` signature, magic-over-extension dispatch, every flag from the right field. 1 new LOW. |

Crates touched this pass: `bsa`, `nif`, `sfmaterial`, `bgsm`, `plugin`, `core`, `renderer`,
`game-detect`, plus `byroredux/`. Not touched (and not claimed as covered): `audio`, `facegen`,
`hkx`, `mod-runtime`, `papyrus`, `pex`, `physics`, `save`, `scripting`, `sdk`, `spt`, `ui`,
`debug-server`/`debug-protocol`, `boot-request`, `settings-io`, `fsr3-sys`.

---

## Deduplication

All six findings were checked against 1,000 all-state GitHub issues
(`/tmp/audit/issues_all.json`) and a keyword scan of `docs/audits/`. No duplicates.
Verified-still-live and deliberately **not** re-reported: #3398 (CDB Phase 2 + the
`read_user_class` declaration-order defect — premise re-confirmed at
`crates/sfmaterial/src/reader.rs:543-612`), #3659 (BA2 mutex across inflate — re-confirmed at
`crates/bsa/src/ba2.rs:400-450`), #2625 (opaque-tail suppresses drift telemetry — re-confirmed
at `crates/nif/src/blocks/shader.rs:793`), #2637, #1576, #3524.
Verified-fixed-and-not-regressed: #1292, #1209, #1828, #1829, #3549, #3777, #1571, #762, #2633,
#1567, #1568, #1291, #1294, #1606, #3396, #3230, #2709, #2643, #2108, #3186, #3349, #3734,
#3814, #2695, #2697.

---

Report ready. Next step:

```
/audit-publish docs/audits/AUDIT_STARFIELD_2026-09-05.md
```

Label every finding `game:starfield` + `legacy-compat`, plus its own domain label
(`import-pipeline` for D7-01, `nifal` for D8-01, `nif` + `test-gap` for D9-01,
`doc-rot` for D8-02 / D8-03, `tech-debt` for D3-01).
