# Starfield Compatibility Audit — 2026-09-11

**Scope**: Depth/correctness regression audit of ByroRedux's Starfield
`GameKind` bring-up surface — BA2 v2/v3 LZ4 decompression, `BSGeometry` mesh
extraction, CDB material database, ESM resolve-rate, ESM+cell bring-up,
NIF shader blocks (BSVER 155+), real-data validation, NIFAL canonical material
translation, and BGSM/BGEM external material flow. **Not** a from-scratch gap
inventory — Starfield already ships as a first-class `GameKind` with NIF + BA2
parsing at 99.98% aggregate (120,524/120,543, recover 100% across 13
mesh-bearing archives per ROADMAP.md), CDB + BGSM/BGEM materials, and a
walkable Cydonia interior.

**Method**: 9 dimension agents (general-purpose / legacy-specialist /
renderer-specialist), each reading live source at HEAD (`f1b39168`,
2026-09-11) independently, cross-checked against `gh issue list` (up to 800
issues pulled across dimensions) and prior `docs/audits/AUDIT_STARFIELD_*.md`
reports for deduplication. Several dimensions ran live verification: BA2
extraction against the real 129-archive Starfield corpus, `--sf-smoke` against
`citycydoniamainlevel` on vanilla `Starfield.esm`, `nif_stats` against two real
mesh archives, and 5 representative real meshes traced through
`import_nif_scene`.

**Result**: **0 CRITICAL, 0 HIGH, 12 MEDIUM, 12 LOW** — 24 findings total, all
NEW except one (DIM1-01) that reconfirms an already-open issue. No regressions
in any of the bring-up milestones (#1289/#1291/#1292/#1294/#1295), no drop in
parse/resolve rates, no BA2 defect, and the four shader-block regression
guards this audit exists to protect (#1510, #1606, #2616, #2622) are all
intact. Two dimensions (5 — ESM+cell bring-up, 7 — real-data validation)
returned clean with zero findings.

---

## Executive Summary

Starfield's Starfield-specific bring-up surface is in good shape six-plus
sessions after the initial 5-day bring-up. Every named prior fix this audit
was tasked with regression-guarding — #1292 (canonical `geometries\` path),
#1828/#1829 (sentinel-slot skip), #3549/#3930 (skin-name recovery, which has
*advanced* past the audit brief's cited figures), #1291 (Starfield XCLL
108-byte body), #1567 (LIGH DAT2 decode), #1510/#1606 (shader-block dispatch +
tail capture), and the BA2 v3 `compression_method`/`packed_size==0` selector —
is present, correct, and in most cases more robustly tested than the brief
assumed. Two long-standing "premise corrections" surfaced independently by
different dimensions: the #3549 skin-name recovery figure (46.8%/36.6%) is
stale in the *good* direction (#3930's `SkinAttach` channel now covers
18,990/18,990 all-NULL-ref skins), and the CDB `BSShaderCRC32` hash derivation
— previously recorded as unrepeatable and explicitly flagged "do not repeat
this search" — is in fact fully reproducible (32/32) once the CRC helper's
hard-coded lowercase fold is removed (finding D6-02).

No finding rises to HIGH or CRITICAL. The MEDIUM findings cluster around three
themes that recur across dimensions rather than one dominant defect:

1. **Silent-degradation risk on malformed/adversarial input** — CDB duplicate
   class names last-win instead of erroring (D3-01), `probe_header` aborts the
   *entire* Starfield PBR gate on one unrecognized chunk FourCC it never
   interprets (D3-02), and the one per-vertex bone-index producer with no
   bound check on `.mesh` input (D2-01). None of these are reachable on
   vanilla retail content today.
2. **Data captured by the NIF parser but never forwarded across the NIFAL
   boundary** — Starfield `wetness`/`luminance` scalars (D8-01), and a doc-rot
   bug (D8-03) where the boundary's own contract comment calls the dominant
   Starfield code path (the NaN backstop, now live for 97.9% of Starfield
   meshes since #2707) "unreachable," inviting a future cleanup to delete it
   and ship NaN into `GpuMaterial`.
3. **A shared flag/field carries two incompatible meanings** across two call
   sites that both read it: `from_bgsm` gates both an FO4-specific BGSM
   convention and (since #2710) an "external material was resolved" glass
   promotion signal; Starfield satisfies the second meaning but is coded to
   the first, so Starfield effect-shader glass can never take the dielectric
   path FO4's identical authoring does (D8-02). A parallel, structurally
   similar bug (D9-01) has the BGSM merge path *assign* rather than *OR* a
   greyscale-palette enable bit, silently clearing a NIF-authored remap.

Also newly resolved by this audit: the CRC32 flag→name derivation (D6-02,
informational + fix), which retires an incorrect "do not repeat this search"
instruction left by a prior audit — a case of the exact failure mode
`_audit-common.md` warns against, corrected here rather than perpetuated.

---

## Dimension Findings

### Dimension 1 — BA2 v2/v3 LZ4 Block Decompression
**Verdict**: No new functional defect. 1 MEDIUM (reconfirms existing #3659,
not new), 1 LOW (NEW).

Live-verified against the real 129-archive Starfield corpus (`--ignored`
sweep, 129/129 opened, 0 failures) plus a standalone deep-extract probe on
9,244 real DX10 texture files (0 errors, valid DDS headers throughout,
including archives large enough to exercise the documented 3.66%
mixed-raw/LZ4-chunk population). Also **directly disproved** the hypothesis
that the BA2 layer causes the ROADMAP's residual 6-file Starfield NIF
truncation tail: re-extracting the three named residual files with
size-mismatch warnings enabled produced zero warnings — the BA2 layer
decompresses them completely and correctly; the truncation is downstream in
NIF parsing (#2105/#3524), not in `ba2.rs`.

| Severity | ID | Title | Status |
|---|---|---|---|
| MEDIUM | DIM1-01 | `Ba2Archive::extract` holds its file `Mutex` across zlib/LZ4 inflate, serializing decompression across concurrent callers | **Existing: #3659** (OPEN) |
| LOW | DIM1-02 | No byte-literal fixture pins the documented mixed-raw+LZ4-chunk DX10 record; only the opt-in real-data sweep would catch a regression | NEW |

### Dimension 2 — BSGeometry Mesh Extraction
**Verdict**: 1 MEDIUM, 3 LOW, all NEW. All ten checklist items verified
clean or clean-with-a-noted-gap; every named prior fix (#1292, #1203, #1209,
#1232, #1828, #1829, #3549, #3777) present and regression-guarded. #3549's
recovery rate has *advanced* past the audit brief (#3930's `SkinAttach`
channel now primary, covering 18,990/18,990 all-NULL-ref skins).

| Severity | ID | Title |
|---|---|---|
| MEDIUM | SF-2026-09-11-D2-01 | `convert_bs_geometry_skin_weights` is the only per-vertex bone-index producer that passes `.mesh` bone indices through unbounded, breaking an invariant `render/skinned.rs` documents as structural |
| LOW | SF-2026-09-11-D2-02 | #3777's `remaining() == 0` trailer gate is undecidable on the inline (Stage A) path; none of its three tests cover it |
| LOW | SF-2026-09-11-D2-03 | #3930 made `SkinAttach` primary but did not short-circuit the now-redundant #3549 geometric solve, which still runs (and is discarded) on ~89.5% of Starfield skinned shapes |
| LOW | SF-2026-09-11-D2-04 | Stage A has none of the #2357 resolve logging Stage B got; the #1232 tangent-synthesis positive arm has no end-to-end test |

### Dimension 3 — CDB Material Database Correctness
**Verdict**: 3 MEDIUM, 3 LOW, all NEW. Parser diffed field-by-field against
the reference Gibbed.Starfield C# implementation — port fidelity is high.
Confirmed CDB Phase 2 unblocked framing (#3398) is current: the lookup key
and field vocabulary are solved; the real blocker is the corpus-wide ~18 GB
parse peak (13 CDBs, two full-size), and the *XMCOLOR* field-offset defect
(#3398) is still live, faithfully ported from the reference's own latent bug.

| Severity | ID | Title |
|---|---|---|
| MEDIUM | SF-2026-09-11-D3-01 | Duplicate CDB class `name_offset` silently last-wins (`HashMap::insert`) where the reference hard-fails — the unfixed sibling of #2633, which fixed the same class of defect for fields |
| MEDIUM | SF-2026-09-11-D3-02 | `probe_header` — the only CDB path production code runs — aborts the entire Starfield PBR gate on any unknown chunk FourCC it never actually interprets |
| MEDIUM | SF-2026-09-11-D3-03 | The real-data CDB test runs the *unlimited* parse (9.19 GB measured peak) with no `ParseLimits` and no memory warning in its own documented run command |
| LOW | SF-2026-09-11-D3-04 | `Field::offset`/`Field::size` are parsed and read by nothing in-tree; no committed guard for the declaration-order-vs-offset-order divergence that found XMCOLOR |
| LOW | SF-2026-09-11-D3-05 | Stale line-citation in `starfield_mat.rs` fixture doc points at `probe_header`'s body instead of the `index_chunks` arithmetic it justifies |
| LOW | SF-2026-09-11-D3-06 | No open tracker covers the loose `.mat` JSON resolver — #762 closed with its named first deliverable (Stage A) unbuilt, and #3398 is CDB-only |

### Dimension 4 — Starfield ESM Resolve-Rate Baseline
**Verdict**: 1 MEDIUM (NEW). Live-verified with a real engine build against
vanilla `Starfield.esm`: `--sf-smoke citycydoniamainlevel` resolved
25,433/27,898 REFRs = **91.2%**, matching the documented Phase 1 baseline
(~91.16%) — no regression. `LIGH` resolves exactly 656 REFRs, confirming the
#1567 DAT2 fix is live in the built binary, not just in test fixtures.

| Severity | ID | Title |
|---|---|---|
| MEDIUM | SF-D4-01 | `sf_smoke`'s hand-maintained `DISPATCH_HANDLED_FOURCCS` byte-coverage list has drifted again — `OMOD`, `LVSP`, `SCEN` are real dispatch arms it still reports as "skip" (cosmetic; no REFRs mis-resolved; same recurring drift class as the prior LCTN drift) |

### Dimension 5 — ESM + Cell Bring-up Regression Surface
**Verdict**: **No findings.** All 7 checklist items (HEDR-0.96 classification,
FourCC dispatch, PDCL conscious skip, XCLL_SIZES_STARFIELD `[28,108]`,
per-cell NAVM collection, five spawn-path guards, structural BLAS exclusion)
verified intact against live code, corroborating the prior
`AUDIT_STARFIELD_2026-09-05b.md` "Clean" result. The two commits since that
report's HEAD are both generic parser-mechanism fixes with zero measured
Starfield occurrence, correctly out of scope.

### Dimension 6 — NIF Shader Blocks, BSVER 155+
**Verdict**: 1 MEDIUM, 2 LOW, all NEW. The four regression guards this
dimension exists to protect (#1510 dispatch, #1606 tail, #2616 alignment,
#2622 luminance) are all intact. See the CRC32 Flag Table below for D6-02's
derivation, which retires a stale "do not repeat this search" instruction.

| Severity | ID | Title |
|---|---|---|
| MEDIUM | SF-2026-09-11-D6-01 | `Own_Emit` additive-blend promotion is typed-word-only, so it can never fire on any CRC-era (BSVER ≥ 132) `BSEffectShaderProperty` — the fifth flag on a block whose other four were fixed by #890, still on the one-sided path. Zero live blast radius on vanilla Starfield (never observed in the corpus); reachable on FO76/mods. |
| LOW | SF-2026-09-11-D6-02 | The `BSShaderCRC32` hash derivation, twice recorded as opaque/unrepeatable ("do not repeat this search"), is fully reproducible (32/32) — the prior negative result hard-coded a lowercase fold that hid the correct uppercase match |
| LOW | SF-2026-09-11-D6-03 | `parse_fo76_plus` keeps a third inline copy of the CRC-array head that #3845's consolidation does not cover, while a neighboring comment claims full coverage (no behavioral defect — the copy is correct, the comment is stale) |

### Dimension 7 — Real-Data Validation
**Verdict**: **No findings.** Parse-rate gate logic matches ROADMAP.md
exactly; two of the 13 archives were live re-measured (LODMeshes,
LODMeshesPatch: 100.00%, exact match). The #2105/#3524 residual truncation
tail (19 files: 6 MeshesPatch + 13 ShatteredSpace-Main01) confirmed unchanged
and the mitigating clamp in `crates/nif/src/blocks/node.rs` confirmed present
in the tree, not just closed on paper. Five representative real meshes
(clutter, ship hull, weapon, landscape rock, character body) traced through
`import_nif_scene` from `Starfield - Meshes01.ba2` — all parsed 100% clean,
zero `NiUnknown`, no new block types.

### Dimension 8 — NIFAL Canonical Material Translation for Starfield
**Verdict**: 3 MEDIUM, 1 LOW, all NEW. The single-boundary invariant
(`translate_material`), the resolve-once plain-`f32` invariant, and the
absence of per-draw/per-game material classification all hold for Starfield
content — confirmed by a shader-source grep finding exactly one per-game
token in any shader (`STARFIELD_WATER_CONCENTRATION_REFERENCE`, itself
D8-04). Also confirmed the particle/per-shape-collision framing (vanilla
Starfield ships zero `NiPSysEmitter*`/`Bhk*Shape` blocks) with a corrected
block-type count (29 distinct types, not 24, per the checked-in baseline).

| Severity | ID | Title |
|---|---|---|
| MEDIUM | SF-2026-09-11-D8-01 | Starfield `BSLightingShaderProperty.wetness` and `.luminance` are parsed but have no `ImportedMaterial` sink — never cross the NIFAL boundary. Live loss is small (97.9% of meshes are stub-shaped; the luminance quad is 100% authored defaults in the sampled corpus) but the structural gap is unrecorded. |
| MEDIUM | SF-2026-09-11-D8-02 | `from_bgsm` is overloaded between two consumers with incompatible meanings (FO4 spec-glossiness convention vs. #2710's "external material resolved" glass-promotion signal); Starfield satisfies the second meaning but reads as the first, so Starfield effect-shader glass (748 blocks) can never take the dielectric path FO4's identical authoring does — becomes live the moment CDB Phase 2 lands texture paths |
| MEDIUM | SF-2026-09-11-D8-03 | `Material::resolve_pbr`'s own contract-doc comment calls its NaN backstop "unreachable," false since #2707 for 97.9% of Starfield meshes (the dominant material-reference-stub case) — risk that a future cleanup deletes the "dead" arm and ships NaN into `GpuMaterial` |
| LOW | SF-2026-09-11-D8-04 | Starfield-specific water-concentration unit convention (`STARFIELD_WATER_CONCENTRATION_REFERENCE`) is normalized in `water.frag` at draw time instead of at the parser→canonical translate boundary — the one per-game token found in any shader source |

### Dimension 9 — BGSM/BGEM External Material Flow
**Verdict**: 2 MEDIUM, 2 LOW, all NEW. The BGEM-vs-BGSM distinction, the
narrow `&mut ImportedMaterial` merge signature, the `.mat`→roles invariant
(pinned pre-emptively for CDB Phase 2), and the BGEM `glass_enabled`
stuck-flag regression test all verified clean and non-vacuous.

| Severity | ID | Title |
|---|---|---|
| MEDIUM | SF-2026-09-11-D9-01 | The BGSM merge arm *assigns* (rather than ORs) the greyscale-palette enable bit when the BGSM fills a role the NIF left empty — silently clears a NIF-authored SLSF1 palette remap. Reachable on any Skyrim-layout BGSM mesh (slot 3 is `Height`, not `GreyscaleLut`, so the NIF side can never fill the role and always takes the clobbering branch) — a third case #3898's "neither source may silently disable the other's remap" fix didn't cover. |
| MEDIUM | SF-2026-09-11-D9-02 | The second, parallel external-material resolver (`RefrTextureOverlay::fill_from_bgsm`) claims exact BGEM parity with `merge_external_material` but drops `base_texture`→diffuse and `envmap_mask_texture`→env_mask, and fills env unconditionally with no `env_mapping_enabled()` gate — reintroducing on this path exactly what #2643 fixed on the merge path |
| LOW | SF-2026-09-11-D9-03 | The guard test for D9-02's regression leaves both dropped fields empty in its fixture, so "every BGEM texture role" test covers 6 of 8 and cannot fail |
| LOW | SF-2026-09-11-D9-04 | The `#[must_use]` `MergeOutcome` is `let _ =`'d at all four production call sites — the `PresenceOnly` signal #2709 created for exactly this purpose still has no telemetry sink, on Starfield ~100% of materials |

---

## CRC32 Flag Table

**Derivation (newly established by Dimension 6, empirically, 32/32 match)**:

> `crc = bscrc32_no_case_fold(NAME)` — reflected CRC-32, polynomial
> `0xEDB88320`, init `0`, **no** final XOR, over the flag name in **ASCII
> uppercase exactly as nif.xml spells it**. Identical parameterisation to
> `crates/bsa/src/csg.rs::bscrc32` and the Starfield CDB key hash — the only
> difference is the case fold (CSG filenames lowercase, shader flags
> uppercase). Equivalent one-liner: `zlib.crc32(NAME, 0xFFFFFFFF) ^
> 0xFFFFFFFF`.

This corrects two standing claims that the derivation is opaque/unrepeatable
(`crates/nif/src/shader_flags.rs:515-524` and
`docs/audits/AUDIT_STARFIELD_2026-08-30.md:288-293`, which explicitly said
"recorded so the search is not repeated"). The prior negative probe reused
`bscrc32`, which hard-codes a lowercase fold internally — varying input case
against that function collapses every variant onto the same lowercase hash,
so the correct uppercase form was never actually tested. See finding
SF-2026-09-11-D6-02.

`crates/nif/src/shader_flags.rs::bs_shader_crc32` names all 32 of 32 nif.xml
`BSShaderCRC32` entries — complete coverage of the spec, 10/10 of the values
that occur in vanilla Starfield. No unknown hash remains; a future unknown
hash is now resolvable by brute-forcing a candidate name list.

| Flag name (nif.xml spelling) | CRC32 (dec) | CRC32 (hex) | Read by import? | Seen in vanilla SF¹ |
|---|---:|---|:-:|:-:|
| `CAST_SHADOWS` | 1563274220 | `0x5D2DABEC` | yes | no |
| `ZBUFFER_TEST` | 1740048692 | `0x67B70934` | yes | 74 |
| `ZBUFFER_WRITE` | 3166356979 | `0xBCBAC5F3` | yes | 74 |
| `TWO_SIDED` | 759557230 | `0x2D45EC6E` | yes | 1 |
| `VERTEXCOLORS` | 348504749 | `0x14C5C2AD` | **no** | 1,396 |
| `PBR` | 731263983 | `0x2B9633EF` | no | no |
| `SKINNED` | 3744563888 | `0xDF3182B0` | yes | 3 |
| `ENVMAP` | 2893749418 | `0xAC7B1CAA` | no | no |
| `VERTEX_ALPHA` | 2333069810 | `0x8B0FD1F2` | no | no |
| `FACE` | 314919375 | `0x12C549CF` | no | no |
| `GRAYSCALE_TO_PALETTE_COLOR` | 442246519 | `0x1A5C2577` | yes | 1 |
| `DECAL` | 3849131744 | `0xE56D16E0` | yes | 10 |
| `DYNAMIC_DECAL` | 1576614759 | `0x5DF93B67` | yes | 10 |
| `HAIRTINT` | 1264105798 | `0x4B58B946` | no | no |
| `SKIN_TINT` | 1483897208 | `0x58727978` | no | no |
| `EMIT_ENABLED` | 2262553490 | `0x86DBD392` | **no — see D6-01** | no |
| `GLOWMAP` | 2399422528 | `0x8F044840` | yes | no |
| `REFRACTION` | 1957349758 | `0x74AAC97E` | no | 1 |
| `REFRACTION_FALLOFF` | 902349195 | `0x35C8C18B` | no | no |
| `NOFADE` | 2994043788 | `0xB2757B8C` | no | 10 |
| `INVERTED_FADE_PATTERN` | 3030867718 | `0xB4A75F06` | no | no |
| `RGB_FALLOFF` | 3448946507 | `0xCD92BF4B` | no | no |
| `EXTERNAL_EMITTANCE` | 2150459555 | `0x802D68A3` | no | no |
| `MODELSPACENORMALS` | 2548465567 | `0x97E67F9F` | yes | no |
| `TRANSFORM_CHANGED` | 3196772338 | `0xBE8ADFF2` | no | no |
| `EFFECT_LIGHTING` | 3473438218 | `0xCF08760A` | yes | no |
| `FALLOFF` | 3980660124 | `0xED440D9C` | no | no |
| `SOFT_EFFECT` | 3503164976 | `0xD0CE0E30` | yes | no |
| `GRAYSCALE_TO_PALETTE_ALPHA` | 2901038324 | `0xACEA54F4` | yes | no |
| `WEAPON_BLOOD` | 2078326675 | `0x7BE0BF93` | no | no |
| `LOD_OBJECTS` | 2896726515 | `0xACA889F3` | no | no |
| `NO_EXPOSURE` | 3707406987 | `0xDCFA8A8B` | no | no |

¹ Occurrence counts from the 108,816-NIF vanilla-Starfield census in
`docs/audits/AUDIT_STARFIELD_2026-08-30.md:262-275`; "no" = not observed.

**Consumption gap** (informational, adjacent to D6-01): 12 of the 32 named
flags are read by the importer; the other 20 are parsed and preserved but
never consulted. `VERTEXCOLORS` (1,396 occurrences — the most common CRC in
Starfield content) and `EMIT_ENABLED` are the two whose absence has an
obvious render-side meaning; only `EMIT_ENABLED` is filed as a finding (D6-01)
because it has a typed-word sibling that IS honoured on other games, making it
a per-game divergence rather than an unbuilt feature.

---

## Remaining-Work Chain

Per `starfield-esm-roadmap.md`: Phases 0+1 done, Phases 2-4 invalidated by the
99.9%-record-parity measurement. In priority order:

1. **CDB Phase 2 — per-field `.mat` extraction** (#3398, single highest-value
   remaining Starfield fidelity item). The lookup key (reflected CRC-32 over
   lowercased directory+stem, `BSResource::ID` field-rotated) and the field
   vocabulary (61 `BSMaterial::*` classes reached, ~20 relevant, tabulated
   against `ImportedMaterial` targets) are **solved** as of 2026-08-29 — do
   not re-describe either as unknown. What remains: an indexed/streaming
   reader that avoids the corpus-wide **~18 GB** parse peak (13 CDBs, **two**
   full-size at ~105 MB/~1.46M chunks each — not the single-CDB 9.19 GB
   figure), and the *XMCOLOR* field-offset fix (`read_user_class` reads
   declaration order, never `Field::offset`; 96 of 97 classes agree, XMCOLOR
   is the one exception). This audit adds two adjacent hardening gaps that
   any Phase-2 reader inherits if unfixed: the duplicate-class-name
   last-wins bug (D3-01) and `probe_header`'s all-or-nothing chunk
   validation (D3-02).
2. **PDCL ahead of GBFM** (per the baseline doc's promote/defer rule: PDCL
   sits unranked at 74.9% of unresolved Cydonia REFRs vs. GBFM's 0.081% —
   ~900× more impactful by the same metric).
3. **Exterior worldspace tiles.**
4. **Space-cell / planet / GBFM records.**
5. **The #2105/#3524 NIF truncation tail** (19 files: 6 MeshesPatch + 13
   ShatteredSpace-Main01, `BSWeakReferenceNode`/`BSWaterReferenceStruct`
   water-ref-count truncation) — confirmed unchanged and the mitigating
   clamp confirmed present in `crates/nif/src/blocks/node.rs` (Dimension 7).

Do not frame this as "BGSM parser first / ESM very far along" — both have
already shipped; the remaining chain above is the accurate ordering.

---

## Total Findings Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 12 |
| LOW | 12 |
| **Total** | **24** |

All findings are NEW except DIM1-01, which reconfirms already-open #3659.
