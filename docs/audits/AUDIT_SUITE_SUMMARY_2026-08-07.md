# Audit Suite Summary — comprehensive — 2026-08-07

21 audits run against HEAD `061dee2f` (renderer/ECS/safety/concurrency/
performance, NIF/NIFAL, audio/speedtree/scripting/save, legacy-compat/
tech-debt, all 6 per-game compat audits, regression, runtime telemetry).

**0 CRITICAL findings across all 21 reports — confirmed.** 19 HIGH findings
this sweep, several of them cross-cutting or independently corroborated by
more than one audit dimension/report. See "Cross-Cutting Findings" below
before reading the per-audit table; those are the highest-value results
from this run.

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | Report |
|---|---:|---:|---:|---:|---:|---|
| Renderer | 65 | 0 | 1 | 22 | 42 | AUDIT_RENDERER_2026-08-07.md |
| ECS | 15 | 0 | 0 | 3 | 12 | AUDIT_ECS_2026-08-07.md |
| Safety | 6 | 0 | 1 | 1 | 4 | AUDIT_SAFETY_2026-08-07.md |
| Concurrency | 8 | 0 | 1 | 3 | 4 | AUDIT_CONCURRENCY_2026-08-07.md |
| Performance | 4 | 0 | 2 | 2 | 0 | AUDIT_PERFORMANCE_2026-08-07.md |
| NIF | 4 | 0 | 0 | 2 | 2 | AUDIT_NIF_2026-08-07.md |
| NIFAL | 4 | 0 | 1 | 2 | 1 | AUDIT_NIFAL_2026-08-07.md |
| Audio | 2 | 0 | 0 | 1 | 1 | AUDIT_AUDIO_2026-08-07.md |
| SpeedTree | 0 | 0 | 0 | 0 | 0 | AUDIT_SPEEDTREE_2026-08-07.md |
| Scripting | 5 | 0 | 1 | 1 | 3 | AUDIT_SCRIPTING_2026-08-07.md |
| Save | 4 | 0 | 1 | 3 | 0 | AUDIT_SAVE_2026-08-07.md |
| Legacy-Compat | 29 | 0 | 0 | 14 | 15 | AUDIT_LEGACY_COMPAT_2026-08-07.md |
| Tech-Debt | 36 | 0 | 0 | 1 | 35 | AUDIT_TECH-DEBT_2026-08-07.md |
| FNV | 9 | 0 | 2 | 1 | 6 | AUDIT_FNV_2026-08-07.md |
| FO3 | 6 | 0 | 0 | 3 | 3 | AUDIT_FO3_2026-08-07.md |
| Skyrim | 17 | 0 | 2 | 4 | 11 | AUDIT_SKYRIM_2026-08-07.md |
| Oblivion | 15 | 0 | 1 | 4 | 10 | AUDIT_OBLIVION_2026-08-07.md |
| FO4 | 19* | 0 | 1 | 5 | 13* | AUDIT_FO4_2026-08-07.md |
| Starfield | 31 | 0 | 5 | 10 | 16 | AUDIT_STARFIELD_2026-08-07.md |
| Regression | 0 | 0 | 0 | 0 | 0 | AUDIT_REGRESSION_2026-08-07.md |
| Runtime | 1 | 0 | 0 | 0 | 1 | AUDIT_RUNTIME_2026-08-07.md |

**Total: 280 findings (0 critical, 19 high, 82 medium, 179 low)**

\* FO4's own closing table states 19 total (13 LOW), but its own
"Breakdown by dimension" line immediately below that table, and the
actual finding list in the body, both sum to **22** (1 HIGH / 5 MEDIUM /
16 LOW) — the closing table undercounts LOW by 3. Reported here as the
report's own stated closing tally per protocol; see "Report Consistency
Notes" below. The grand total in this summary uses the stated 19/13, so
the true corpus-wide total is likely **283**, not 280 — flagged, not
silently corrected.

---

## Cross-Cutting Findings (highest priority — read these first)

### Starfield: 3 real HIGH shader/skin bugs + CDB allocation-safety gaps

Starfield produced the densest and most consequential result set of the
sweep (5 of 19 HIGH findings, one report alone):

1. **Bind-pose skinning — `SF2D2-D2-01`** (independently found by 2 audit
   dimensions). `extract_skin_bs_geometry`
   (`crates/nif/src/import/mesh/skin.rs:233-275`) hardcodes empty
   `vertex_bone_indices`/`vertex_bone_weights` even though
   `BSGeometryMeshData.skin_weights` is already decoded and in scope at
   the call site (`bs_geometry.rs:249-260`) — it's simply never passed
   in. Dimension 2 found the source-level gap; Dimension 7 independently
   corroborated it against two real vanilla meshes (`naked_f.nif`,
   6,616 verts/38 bones; `femalehead_facebones.nif`, 15,370 verts/50
   bones), both showing `vbi_len=0 vbw_len=0`. Every Starfield NPC,
   creature, and skinned apparel/armor mesh renders in bind pose. The
   stale #1827 (CLOSED) tracker and a stale test assertion
   (`bs_geometry_skin_tests.rs:118-121`) both need revisiting.

2. **Shader word-misalignment — `SF-D6-01`**. `parse_fo76_plus`
   (`crates/nif/src/blocks/shader.rs:1142-1161`) makes two compensating
   4-byte errors on Starfield: it skips a `shader_type` u32 Starfield
   actually carries, then unconditionally reads a `root_material_path`
   Starfield does not carry. Total byte-consumption stays correct (so
   parse-rate telemetry never flags it), but every field between the two
   errors — CRC arrays, UV offset/scale, `texture_set_ref`, emissive
   color/multiple — is read one word early. Corpus-wide corrected-
   alignment scoring: **0/2,538 full-body blocks valid under the shipped
   alignment, 2,538/2,538 valid under the corrected one** — this affects
   ~100% of Starfield's non-stub `BSLightingShaderProperty` blocks, with
   57% of them also getting an invalid CRC flag that corrupts downstream
   decal/two-sided/PBR/vertex-color classification.

3. **Effect-shader invisibility — `SF-D8-2026-08-07-01`**. The #2353
   material-reference-stub guard was added to the
   `BSLightingShaderProperty` walker but never to its sibling
   `apply_bs_effect_shader` (`crates/nif/src/import/material/dedicated_shader.rs:365-500`).
   On Starfield the stub discriminator ("name is non-empty") is the
   *dominant* authoring path, since materials live in
   `materialsbeta.cdb` and are referenced by name. Ungated, the stub's
   placeholder falloff values drive `finalAlpha` to 0.0 in
   `triangle.frag:790-799` — every externally-referenced Starfield
   effect-shader surface renders fully transparent, with no error or log
   line.

4. **CDB allocation safety — `SF-D3-01`/`SF-D3-02`** (Dimension 3, not
   cross-referenced to `AUDIT_SAFETY`, but the same finding class as
   Safety's own `SAFE-2026-08-07-01` below).
   `index_chunks` (`crates/sfmaterial/src/reader.rs:172-179`)
   pre-reserves a `VecDeque` sized directly from an unvalidated on-disk
   `u32` chunk count *before* the per-chunk overflow guard runs — a
   corrupt/truncated `materialsbeta.cdb` can request ~103 GB and
   abort the process (HIGH; confirmed on the live `register_starfield_cdb`
   path used on every cell load). A sibling `LIST`/`MAPC` count-as-`i32`
   cast bug (`SF-D3-02`) is currently unreachable (Phase 2 CDB extraction
   isn't wired up yet) but becomes live — and should be fixed in the same
   patch — the moment #2359/#1289 Phase 2 lands.

### FO4: `#973` material-swap feature is a near-total no-op in production

**`FO4-D6-001` (HIGH)** — `RefrTextureOverlay.material_swaps`
(`byroredux/src/cell_loader/refr.rs:76`) is computed correctly at
overlay-build time but has zero production consumers:
`spawn.rs::resolve_mesh_paths` never reads it. A real-data probe this
session (`XMSP total=152086 with_xato_or_xtnm=2 alone=152084`) shows the
only path that currently applies any swap (#971's eager single-shape
substitution) fires for just 2 of 152,086 vanilla XMSP-bearing REFRs.
For the other **152,084 (99.9987%)**, the swap table is built and
discarded unread — effectively the entire vanilla MSWP feature (Raider
armor color variants, settlement clutter recolors, vehicle rust
patterns, Vault decay overlays) renders with unswapped base materials.
**Recommendation: reopen #973 and relabel it from `low` to `high`**
rather than filing a new issue — the code hasn't changed since #973's
2026-05-24 closure, only the real-world blast-radius assessment has.
#973's own suggested fix (thread `ov.material_swaps` +
`MaterialSwapRecord.path_filter` into `spawn.rs`'s per-shape loop)
remains valid.

### Skyrim: two independent HIGH bugs, each with a large measured blast radius

- **`SK-D1-01`** — `decode_sse_skin_payload`
  (`crates/nif/src/import/mesh/skin.rs:397-415`) still calls the
  position-dependent `decode_sse_packed_buffer` wrapper instead of the
  `_with_external_positions` variant, so it bails whenever `VF_VERTEX`
  is clear with no external positions supplied — true of every vanilla
  FaceGen partition buffer. This is a residual of #2318, which fixed the
  *geometry* half of `BSDynamicTriShape` decoding but not the sibling
  skin-weight decode. Measured: 21,139 of 21,140 `BSDynamicTriShape`
  blocks with a populated `skin_ref` have both skin weights and indices
  missing — **78% of all skinned Skyrim SE geometry (21,139 of 26,940
  shapes)**. Every Skyrim SE/AE NPC's head, eyes, brows, mouth, and
  hair-cap render rigid, parented to the placement root instead of
  skinned to `NPC Head`/`NPC Neck`.
- **`SK-D6-01`** — Both LOD path builders (`object_lod.rs:385-400`,
  `terrain_lod.rs:273-277,367-380`) derive a quad's SW-corner cell as
  `cell.div_euclid(level) * level`, assuming every worldspace's LOD grid
  is aligned to absolute multiples of `level`. True only for Tamriel and
  Solstheim. At the only level either loader requests: 5,735/7,897 files
  resolvable (72.6%) overall, but **9 of 12 vanilla worldspaces resolve
  zero** — including Apocrypha (Dragonborn's main questing space, 1,063
  LOD files) and Soul Cairn (Dawnguard, 944 files) — permanently and
  silently, with no log line and no fallback beyond flat-texture terrain
  synthesis.

### Oblivion: legacy particle stack parses but is structurally unreachable

**`OBL-D4-01` (HIGH)** — The particle-emission site
(`crates/nif/src/import/walk/mod.rs:531-568`) downcasts only to the
modern `NiParticleSystem`/`NiPSysEmitter*` shape. Oblivion's *other*
particle stack — `NiParticleSystemController`/`NiBSPArrayController` +
`NiAutoNormalParticles`/`NiRotatingParticles`, whose own dispatcher
comment reads "Oblivion magic FX, fire, dust, blood" — dispatches to
`legacy_particle::*` and matches neither downcast, so it imports with
zero emitters. `legacy_particle::NiParticleSystemController` already
decodes a superset of the needed spawn parameters (speed, declination,
birth_rate, lifetime, emitter_dimensions) — it's parsed, just never
consumed. A prior audit had marked this "confirmed dead code"; this pass
reverses that call. Every Oblivion FX asset using the legacy stack —
torch fire, magic-effect shaders, dust, blood, smoke — renders as static
geometry with zero particles, with no parse error to signal it.

### Other significant findings this sweep

- **`SCR-D6-NEW6-01` / `SAVE-D6-01` — independently found by two separate
  audits (Scripting and Save), same underlying bug, HIGH.**
  `QuestAliasInjectionState`'s permanent inventory-grant ledger
  (`inventory_grants`) is keyed by raw, session-local `EntityId`, which
  is not stable across a live in-session cell reload (monotonic
  entity-id allocation means post-reload actors never match ledger
  entries). Quest-alias-injected inventory items (CNTO grants) are
  silently re-granted/duplicated on every live in-session reload of the
  affected cell — a real, repeatable item-duplication exploit via the
  ordinary `load` command, not cosmetic. The Scripting audit explicitly
  defers to Save as the canonical tracking issue rather than re-filing —
  this is why it appears once in each report's own tally (contributing
  2 to this sweep's 19-HIGH count) but is one bug, not two.
- **`NIFAL-D3-NEW-01` (HIGH)** — the loose-NIF load path
  (`cargo run -- mesh.nif`, and the NPC-part skeleton/body/hand cache
  behind it) never calls `import_nif_lights`, so any standalone-loaded
  NIF with embedded lights (torch, candle, lantern, streetlamp) renders
  the light-emitting geometry but contributes zero actual light to the
  scene. Only the cell-loader path (fixed under #156) extracts/spawns
  lights.
- **`FNV-D2-01` / `FNV-D3-01` (both HIGH, reference-title regressions)**
  — FNV is the compat baseline, so these are especially high-signal:
  `Material.specular_color` faithfully carries FNV's universally-black
  `NiMaterialProperty.specular` through the NIFAL boundary, zeroing the
  direct-specular BRDF term on every FNV surface; and `shadowFade`
  multiplies the *entire* ReSTIR-DI direct-light estimate rather than
  just the shadow term, so every shadow-casting FNV light switches fully
  off (instead of un-shadowing) past ~171 m — masked in the bench-of-
  record because the reference scene never crosses that distance.
- **`SAFE-2026-08-07-01` (HIGH)** — `synthesize_packed_havok_proxy`
  (from `716b7ee9`, packed-collision-compat) can build a collider with
  unbounded/infinite half-extents from an unclamped REFR `XSCL` scale on
  FO4+/FO76/Starfield content; the only guard is a `debug_assert!`
  compiled out of release builds, so a crafted/malformed plugin can
  corrupt physics engine-wide in a release build. Same finding class as
  Starfield's CDB allocation-safety gaps above — unvalidated on-disk
  values reaching unguarded allocation/geometry paths is the recurring
  pattern this sweep.
- **`CONC-D3-2026-08-07-01` (HIGH)** — animation color/float channel
  sinks acquire up to 6 `QueryWrite` guards in an order dictated by
  NIF/KF-authored channel order rather than code, making ECS
  lock-acquisition order content-determined for the first time in the
  audited surface — trips the ABBA detector under
  `BYRO_LOCK_ORDER_CHECK=1`, latent-deadlock-capable if a second
  parallel system is ever added to that stage.
- **`PERF-D6-NEW-01` (HIGH)** — regression of closed #1791/#1796: the
  rollback of drained-but-undispatched skin uploads is wired into only
  the `Ok` match arm in `main.rs`, not the `Err` arm — four early-`Err`
  paths in `draw.rs` can still permanently corrupt an entity's skinning
  data.
- **`PERF-D8-NEW-01` (HIGH)** — `allocate_vec<T>`'s bounds check treats
  every element as 1 byte regardless of `size_of::<T>()`; a corrupt/
  hostile NIF count can amplify into multi-gigabyte (up to ~19 GB in the
  worst case) allocation requests that abort the process — a DoS vector
  shared across ~20 call sites.
- **`AS-D1-NEW-01` (HIGH)** — the BLAS-scratch-shrink peak-walk ignores
  live *skinned* BLAS scratch, so the scratch buffer can shrink below
  what a resize/reload actually needs.

---

## Report Consistency Notes

- **FO4's closing tally undercounts by 3 LOW** (19 stated vs. 22 actual
  per the report's own dimension breakdown and finding list — see the
  `*` footnote on the table above). Not corrected in this summary's
  numbers per protocol (trust each report's own stated tally), but
  flagged for the report owner to fix before publishing.
- **Scripting's "5 new findings" headline vs. its own by-dimension
  breakdown ("Dim 6 — 2 MEDIUM")** — a naive reader summing the body's
  `### MEDIUM` subsections gets 2, not the headline's 1. This is
  self-explained in the report: `SCR-D6-NEW6-01` is deliberately excluded
  from the "5 new" headline because it's the same bug as `SAVE-D6-01`
  (see Cross-Cutting Findings above), tracked canonically under Save.
  Not a real arithmetic error, but worth a proofread pass.
- All other 19 reports' closing tallies were spot-checked against their
  own body finding lists and found internally consistent (per-dimension
  breakdowns and severity tag counts sum to the stated totals exactly).

---

## Process Note

This run experienced several full session interruptions/crashes over its
multi-hour wall-clock span (per-audit report timestamps range from
17:18 to the following day at 00:10), and involved substantial manual
bridging of nested-agent relay results — background audit agents that
themselves fan out into per-dimension sub-agents can't retrieve their own
children's results in this environment, so several dimensions' findings
had to be recovered from `/tmp/audit/<name>/dim_N.md` scratch files and
cross-checked against the final written reports before this summary could
trust them. All in-flight work was recovered from disk/agent transcripts
with no data loss, though it materially extended wall-clock time.

---

## Suggested Next Steps

For each report with findings, file them as GitHub issues:

```
/audit-publish docs/audits/AUDIT_RENDERER_2026-08-07.md
/audit-publish docs/audits/AUDIT_ECS_2026-08-07.md
/audit-publish docs/audits/AUDIT_SAFETY_2026-08-07.md
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-08-07.md
/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-08-07.md
/audit-publish docs/audits/AUDIT_NIF_2026-08-07.md
/audit-publish docs/audits/AUDIT_NIFAL_2026-08-07.md
/audit-publish docs/audits/AUDIT_AUDIO_2026-08-07.md
/audit-publish docs/audits/AUDIT_SCRIPTING_2026-08-07.md
/audit-publish docs/audits/AUDIT_SAVE_2026-08-07.md
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md
/audit-publish docs/audits/AUDIT_TECH-DEBT_2026-08-07.md
/audit-publish docs/audits/AUDIT_FNV_2026-08-07.md
/audit-publish docs/audits/AUDIT_FO3_2026-08-07.md
/audit-publish docs/audits/AUDIT_SKYRIM_2026-08-07.md
/audit-publish docs/audits/AUDIT_OBLIVION_2026-08-07.md
/audit-publish docs/audits/AUDIT_FO4_2026-08-07.md
/audit-publish docs/audits/AUDIT_STARFIELD_2026-08-07.md
/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-07.md
```

(`AUDIT_SPEEDTREE_2026-08-07.md` and `AUDIT_REGRESSION_2026-08-07.md` came
back fully clean — zero findings, nothing to publish.)

Given the cross-cutting nature of the Starfield CDB / Safety
`synthesize_packed_havok_proxy` finding pair, and the Scripting/Save
shared `EntityId`-instability bug, consider triaging those as single
issues with a shared root cause rather than duplicating tracking across
reports.
