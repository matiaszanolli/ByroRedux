# #3929: FO4-2026-09-05b-D7-01: two `Material` field docs describe a pipeline stage that has since landed

Filed from `docs/audits/AUDIT_FO4_2026-09-05b.md` (FO4-2026-09-05b-D7-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `low,game:fo4,legacy-compat,nifal,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3929 --json state`.

---

**Source**: `docs/audits/AUDIT_FO4_2026-09-05b.md` (FO4-2026-09-05b-D7-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: 7 (NIFAL canonical translation) / doc-rot
- **Location**: `crates/core/src/ecs/components/material.rs` —
  the doc comments on `Material::greyscale_texture` and
  `Material::grayscale_to_palette_scale`
- **Status**: NEW. `#3903` and `#3899` (the earlier pass's D2-03 / D2-02) are
  filed and unrelated; no open doc-rot issue names these fields.
- **Description**: Two claims in a canonical-contract type are now false.
  * `greyscale_texture`'s doc says it is the *"`BSEffectShaderProperty.greyscale_texture`
    path (Skyrim+)"* and *"`None` for every non-BSEffect mesh."* Since `#2997`
    routed FO4 `BSShaderTextureSet` slot 3 into `TextureRole::GreyscaleLut`, and
    `#3897` began enabling the remap from a `BSLightingShaderProperty`, it is
    populated on 30 166 measured vanilla FO4 **lit** properties. The
    "every non-BSEffect mesh" claim is wrong by five orders of magnitude.
  * `grayscale_to_palette_scale`'s doc says *"Captured here, not yet shaded —
    `triangle.frag`'s palette branch still performs an unmodulated direct lookup,
    and the `GpuMaterial` slot plus the multiply in its
    `MAT_FLAG_EFFECT_PALETTE_COLOR` block are a separate, independently-reviewable
    follow-up."* The `GpuMaterial` slot exists (offset 420, pinned by
    `crates/renderer/src/vulkan/material_tests.rs`) and **both** shader branches
    read `mat.grayscaleToPaletteScale`. The follow-up landed; the doc still
    parks it.
- **Evidence**: `crates/renderer/shaders/triangle.frag` reads
  `mat.grayscaleToPaletteScale` in the effect branch and again in the lit branch;
  `crates/renderer/shaders/include/bindings.glsl` declares
  `float grayscaleToPaletteScale;`; `byroredux/src/render/static_meshes.rs`
  populates it. Measurement section 4 supplies the FO4 lit-path count.
- **Impact**: `docs/engine/nifal.md`'s "parked passthrough" inventory inherits
  the stale entry, so a reader auditing "which canonical fields have no
  consumer" gets a wrong answer for this one — and the same doc is the
  reasoning an auditor would use to *not* look at the shader, which is where
  D5-01 lives.
- **Related**: `#2443`, `#2592` (SKY-D7-04 — the previous correction to this
  same doc paragraph), `#2997`, `#3897`, D5-01 above.
- **Suggested Fix**: rewrite both paragraphs against the live chain; state that
  the field is source-agnostic (BGSM/BGEM **and** `BSLightingShaderProperty`)
  and shaded, and remove the entry from the `nifal.md` parked list. Fold the
  semantic correction from D5-01 into the same edit so the doc does not go on
  describing it as a strength/blend parameter.

---

## Verified clean this run (no findings)

Only items **this** pass re-checked or measured. The earlier pass's much larger
regression-pin inventory (Dimensions 1/2/3/4/5/6/7/8) stands and is not
duplicated here.

### Dimension 1 — M49 precombined geometry
- All 6 present vanilla `.csg` archives open; 69 869 declared objects total
  (measurement 3). *Observation, not a finding*: `#3641`'s doc cites "46 422
  measured CSG objects" while the six headers declare 69 869. `num_objects()`
  is documented as the CK index size, which need not equal the count of
  shared-geometry objects a cell walk actually reaches, so the two numbers are
  not necessarily in conflict — but `#3641`'s figure has no stated provenance
  and could not be reproduced from the headers here.
- `#3758` `read_psg` bound live: an over-length read is rejected before any
  chunk work, and `psg_len()+1` (which sits inside `chunks.len() *
  CSG_CHUNK_SIZE` because the final chunk is short) correctly falls through to
  the documented per-chunk `local >= chunk.len()` check. Behaves exactly as the
  comment says it should.
- `DLCworkshop02` ships no CSG; `open_geometry_csg` returns `None` on the
  `is_file()` guard. Live vanilla fallback, no panic.

### Dimension 2 — BGSM / BGEM
- `cargo test --release -p byroredux-bgsm`: **30 passed / 0 failed** (+2 doc
  tests, +2 integration).
- `#3898`'s three-way split re-read: capture of `nif_supplied_greyscale_lut`
  happens **before** the template walk, so "a closer BGSM won the slot
  mid-walk" stays distinguishable from "the NIF won it outright", and the
  ancestor-shadowing behaviour `#2108` established is preserved. The OR is
  one-way in both arms. Correct as written.
- 477 of 8 330 vanilla BGSMs enable the palette remap, and **all 477** supply a
  LUT texture — no enable-without-resource case exists in vanilla FO4.

### Dimension 3 — BA2 reader
- DX10 validated with real data for the first time: measurement 2. 2 118
  textures, 32 archives, zero failures of any kind. Covers v1 (`Textures1..9`,
  DLC) and v8 (`TexturesPatch`), GNRL-free.
- GNRL side re-confirmed incidentally: 235 082 mesh entries extracted across the
  8 mesh archives with 0 extract errors during measurement 1.

### Dimension 4 — NIF BSVER 130 / half-float / collision
- **`NiSkinPartition` strip-bound regression does not reach FO4** —
  measurement 1. Zero blocks of that type exist in the FO4 corpus; the format
  moved to `BSSkin::Instance` / `BSSkin::BoneData` at BSVER 130. FO3 owns the
  defect (`AUDIT_FO3_2026-09-05.md` `FO3-2026-09-05-D2-01`); FNV and Oblivion
  carry it; FO4 does not, and no FO4-side guard is warranted.

### Dimension 5 — FO4 shader flags
- `#3897`'s claim that no layout dispatch is needed **holds**:
  `fo4_slsf1::GREYSCALE_TO_PALETTE_COLOR` = `0x10` and
  `GREYSCALE_TO_PALETTE_ALPHA` = `0x20` are byte-identical to their
  `skyrim_slsf1` counterparts (`crates/nif/src/shader_flags.rs`), so reusing the
  `skyrim_slsf1` constants on the FO4 lit path is correct, not a shortcut.
- `cargo test --release -p byroredux-nif --lib material`: **264 passed / 0
  failed**.
- The full producer chain for the enable bit is now closed and was traced
  end-to-end: `BSLightingShaderProperty.shader_flags_1` →
  `MaterialInfo::palette_color` / `palette_alpha`
  (`crates/nif/src/import/material/dedicated_shader.rs`) →
  `ImportedMaterial.bgsm_greyscale_lut_{enabled,color,is_alpha}`
  (`MaterialInfo::into_imported_material`) → `pack_imported_material_flags`
  (`byroredux/src/cell_loader.rs`) → `EFFECT_PALETTE_COLOR` /
  `EFFECT_PALETTE_ALPHA` → shader. The earlier pass's HIGH finding
  `FO4-2026-09-05-D5-01` is **fixed**, not merely mitigated.

### Dimension 9 — real-data validation
- Corpus totals reproduce the checked-in expectations exactly (235 082 across 8
  archives, per-archive to the file). The FO4 parse-rate figure was **not**
  re-run as a full `parse_real_nifs` sweep this pass — it was re-run earlier
  today at `fa5c4191` (100.00 % clean, 235 082) and no commit since then touches
  `crates/nif/src/blocks/` or `crates/nif/src/import/mesh/`. Cite that run, not
  this one, for the parse rate.

---

## Cross-references and dispositions

| Prior finding | Disposition at `b3e27d2c` |
|---|---|
| `AUDIT_FO4_2026-09-05.md` `FO4-2026-09-05-D5-01` (HIGH, palette enable bit never set) | **Fixed** by `79194306` (`#3897`). Verified end-to-end this pass. Its activation is the precondition for D5-01 below it. |
| `AUDIT_FO4_2026-09-05.md` `FO4-2026-09-05-D2-01` (MEDIUM, BGSM enable bit dropped when the NIF won the slot) | **Fixed** by `79194306` (`#3898`). Three-way split re-read and correct. |
| `AUDIT_FO4_2026-09-05.md` `FO4-2026-09-05-D2-02` (`peek_magic` re-extract) | Open as **`#3899`**. Not re-litigated. |
| `AUDIT_FO4_2026-09-05.md` `FO4-2026-09-05-D2-03` (role walk without exhaustiveness guard) | Open as **`#3903`**. Not re-litigated. |
| `AUDIT_FO3_2026-09-05.md` `FO3-2026-09-05-D2-01` (`NiSkinPartition` strip bound) | **Does not reach FO4** — 0 blocks in 235 082 entries. FO3 keeps ownership. |
| `#3637` archive-chain precedence, `#3510` precombine dedup, `#3641` LOD tie-break, `#1188` REFR de-dup, `#1476` metalness-from-saturation, `#1823` blend pass-through, `#1148` template cycle break | Verified intact by the earlier pass at `fa5c4191`; no commit since touches those sites. Not re-verified here. |

---

## Forward Scope Chain

Unchanged. In dependency order:

1. `_precomb.nif` collision (`#3809`) + `.uvd` occlusion volumes (`#3810`) — the
   two remaining halves of the precombine story; the geometry half (CSG) is
   shipped.
2. MOVS physics runtime — the record parses; nothing drives it.
3. Deeper cell coverage — LIGH power state, CONT leveled items, NPC_ face morph.
4. FaceGen NIF truncation tail.
5. `BSBehaviorGraphExtraData` — parse-only, nothing pretends to drive it.

Deferred-consumer gaps inside shipped subsystems, documented at both ends and
**not** re-filed: `lighting`/`flow`/`wrinkle` roles unsampled by
`triangle.frag` (`#2712`), BGSM `distance_field_alpha_texture` with no role
(`#2642`), the eleven BGSM scalars with no `ImportedMaterial` sink (`#2704`),
the `anisotropic` strength scalar no source format supplies (`#3613`).

---

## Audit hygiene note

Seven temporary census `examples/` were written to produce every number in this
report (`tmp_fo4_skinpart_census`, `tmp_fo4_lsp_palette` under
`crates/nif/examples/`; `tmp_fo4_dx10_check`, `tmp_fo4_csg_check`,
`tmp_fo4_lut_dims`, `tmp_fo4_bgsm_palette`, `tmp_fo4_lut_rows` under
`byroredux/examples/`). **All seven were deleted before this report was
written**; their sources are preserved outside the repo at
`/tmp/audit/fo4/census_src/`. No engine process was launched, and **no
production file was modified at any point during this audit** — the only tree
change this pass contributes is this report. Everything else `git status` shows
belongs to work running concurrently and was deliberately left untouched: the
sibling per-game reports from the same suite, the Starfield audit's `tmp_sf_*`
scratch examples, and an in-progress `#3850` edit across five `tests/` files
(`byroredux/tests/skinning_e2e.rs`, `crates/bsa/tests/ba2_real.rs`,
`crates/bsa/tests/bsa_real.rs`, `crates/plugin/tests/parse_real_esm.rs`,
`crates/spt/tests/parse_real_spt.rs`).

---

Suggested next step:

```
/audit-publish docs/audits/AUDIT_FO4_2026-09-05b.md
```

Label every finding `game:fo4` + `legacy-compat`, plus its own domain label
(D5-01 → `shaders` + `renderer`; D9-01 → `test-gap`; D7-01 → `doc-rot` +
`nifal`).

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
