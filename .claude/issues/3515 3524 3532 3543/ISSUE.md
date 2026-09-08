# #3515: FO4-2026-08-27-D5-02: the same texture_clamp_mode field carries two different defaults across the three material tiers

Labels: bug, nif-parser, low, legacy-compat, nif, game:fo4, nifal

- **Severity**: LOW
- **Dimension**: 5 (FO4 shader flags) ∩ 7 (NIFAL canonical translation)
- **Location**: `crates/nif/src/import/material/mod.rs:1075` and `:1202` (`MaterialInfo`, default `3`) vs. `crates/nif/src/import/types.rs:643` (`ImportedMaterial`, default `0`) vs. `crates/core/src/ecs/components/material.rs:551` (`Material`, default `0`)
- **Source**: `docs/audits/AUDIT_FO4_2026-08-27.md` — finding `FO4-2026-08-27-D5-02`

## Description

`3` is `WRAP_S_WRAP_T`, the Gamebryo default that `#610` established and that `resolve_texture` hardcodes for its clamp-unaware variant (`byroredux/src/asset_provider/texture.rs:285-297`, "3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT default"). `0` is `CLAMP_S_CLAMP_T`, the *opposite* end of the enum.

`MaterialInfo` — the tier the NIF walker actually fills — uses `3`; the two tiers below it default to `0`, and `Material::texture_clamp_mode`'s own doc rationalises this as mirroring "that struct's own `0` (CLAMP_S_CLAMP_T) parser-stub default" (`material.rs:388-390`).

Today the divergence is inert on every real path because `into_imported_material` overwrites the field verbatim (`mod.rs:1455`) and the only production `ImportedMesh::from_geometry` consumers that keep `ImportedMaterial::default()` are the fog volumes (`byroredux/src/fog.rs:1151`, `:1176`), which are untextured. The FO4 precombine path — named in `from_geometry`'s own doc as its other production consumer (`types.rs:844-846`) — is safe only because `into_imported_mesh` reassigns `mesh.material` immediately afterwards (`crates/nif/src/import/precombine.rs:84`).

## Evidence

The three literals above; `resolve_texture_with_clamp`'s registry contract, where `0` selects the CLAMP/CLAMP sampler and out-of-range values fall back to `3` (`texture.rs:298-305`, `crates/renderer/src/texture_registry.rs:171-183`).

## Impact

Latent. The next synthetic-geometry producer that builds through `ImportedMesh::from_geometry` and *does* bind a tiling texture — distant object LOD, terrain LOD, a future `_precomb.nif` collision-visual path — will silently get CLAMP/CLAMP on a tiling atlas and read as one stretched edge texel per axis. The wrong default is also load-bearing documentation: the `Material` doc currently teaches the reader that `0` is the field's default, which contradicts `#610`'s rule.

## Related

- `FO4-2026-08-27-D5-01` (filed as its own issue) — the live half of the same field
- `#610`
- `#2571` / OBL-D5-01 — which propagated the `0` down to `Material`

## Suggested Fix

Set `ImportedMaterial::default().texture_clamp_mode = 3` and `Material::default().texture_clamp_mode = 3`, matching `MaterialInfo` and `resolve_texture`'s own hardcoded fallback, and correct the two doc comments. Note this is a saved `Material` field (`FORMAT_MAJOR` 6 made it required, `crates/save/src/snapshot.rs:65-67`) but changing a *default* does not change the serialised shape, so no format bump is needed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — other fields whose default diverges across the `MaterialInfo` → `ImportedMaterial` → `Material` tiers
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# #3524: SF-2026-08-27b-D7-01: the six residual MeshesPatch truncations are all BSWeakReferenceNode — the #2105 tail is characterised, not unexplained

Labels: bug, nif-parser, medium, legacy-compat, nif, game:starfield

From `docs/audits/AUDIT_STARFIELD_2026-08-27b.md` (branch `main` @ `969d81c8`).

- **Severity**: MEDIUM
- **Dimension**: 7 — Real-data validation (root cause in Dim 6's block-parser territory)
- **Location**: `crates/nif/src/blocks/node.rs` — `BsWeakReferenceNode::parse_inner` (the `SF_WEAK_REF_GAP` 2-byte skip, `unk_int1`, and the water-reference loop that follows it)

## Description

Two prior audits recorded the six residual `Starfield - MeshesPatch.ba2` truncations as a stable-but-unexplained family, explicitly "distinct unexplained cause from the closed `BSWeakReferenceNode` / cloth / `BSShaderType155` tails". They are not distinct. **All six are `BSWeakReferenceNode`**, all at `user_version_2 == 175` (i.e. at-or-above `SF_WEAK_REF_GAP`, so the #2105 2-byte skip *is* applied), and all six drop to `NiUnknown` inside the same water-reference loop.

This is the remainder of #2105's fix, not a regression of it.

## Evidence

Measured against `Starfield - MeshesPatch.ba2`:

| File | block | size | consumed | failure |
|---|---|---|---|---|
| `meshes\terrain\cydoniacity\objects\cydoniacity.4.-2.-2.nif` | 0 | 150 324 | 150 314 | `skip(80)` past EOF |
| `meshes\terrain\sb004templeworld\objects\sb004templeworld.1.-1.0.nif` | 1 | 14 764 | 14 754 | `skip(80)` past EOF |
| `meshes\terrain\lc174world\objects\lc174world.1.0.1.nif` | 1 | 208 | 174 | `skip(1634533376)` |
| `meshes\terrain\cydoniacity\objects\cydoniacity.8.-6.-6.nif` | 0 | 302 052 | 302 042 | `skip(80)` past EOF |
| `meshes\terrain\cydoniacity\objects\cydoniacity.1.-1.-1.nif` | 1 | 14 284 | 14 274 | `skip(80)` past EOF |
| `meshes\terrain\cydoniacity\objects\cydoniacity.2.-2.-2.nif` | 0 | 35 860 | 35 850 | `skip(80)` past EOF |

1. **Five of six stop at exactly `block_size − 10`.** `80 = 64 + 12 + 4` is the water-reference struct skip, so the parser read `num_water_refs` as a non-zero garbage value 10 bytes before the block end.
2. **The 10 bytes are byte-regular across files.** Hex at the block tail, immediately after the last weak-ref entry's `num_materials = 0`:
   ```
   cydoniacity.1.-1.-1.nif   01 00  33 00  cb c0 1a 00  00 00 | 00 00 00 00 00 00 00 00 00 00
   cydoniacity.2.-2.-2.nif   01 00  6a 00  cb c0 1a 00  00 00 | 00 00 00 00 00 00 00 00 00 00
   cydoniacity.4.-2.-2.nif   01 00  d8 00  cb c0 1a 00  00 00 | 00 00 00 00 00 00 00 00 00 00
   ```
   `u16 = 1`, then a per-file-varying `u16`, then the **constant** `u32 0x001AC0CB` (1 753 291) in all three sampled files, then two zero bytes. The `[2-B gap][unk_int1 = 0][num_water_refs = 0]` triple then lands exactly on the block end, which is self-consistent.
3. **A clean sibling proves the run is conditional, not universal.** `meshes\terrain\cydoniacity\objects\cydoniacity.4.-6.2.nif` — same directory, same `user_version_2 = 175`, same `BSWeakReferenceNode` — has only `[gap 2][unk_int1 4][num_water_refs 4] = 10` bytes after `num_materials`, no 10-byte run, and parses clean.
4. **The sixth file diverges earlier.** `lc174world.1.0.1.nif` attempts `skip(1634533376)`; `1634533376 == 0x616D0000`, i.e. the ASCII bytes `\0 \0 m a` — the parser is misaligned *inside* a `materials\…` null-terminated string in the `UnkMaterialStruct` loop (`read_past_cstring`). A distinct sub-mode of the same block type, worth separating in any fix.

**Attempts to disprove** (all failed): the files are not corrupt (the BA2 extract succeeds and the header `block_sizes` sum + 8-byte footer accounts for the file exactly); the #2105 gate is not mis-applied (all six are bsver 175, the gate's own attested boundary, and removing the skip moves the misread *further* off); the outer recovery is working as designed (`truncated == false`, `dropped_block_count == 0`, only `recovered_blocks == 1`), so this is content loss, not stream corruption.

## Impact

Six `BSWeakReferenceNode` blocks — Starfield's composite-LOD / packin reference nodes — are replaced with `NiUnknown`, so their entire weak-reference payload (the terrain-object LOD placements) is dropped. Four of the six are **Cydonia** terrain-object LOD tiles, i.e. the flagship walkable cell. Blast radius is bounded and non-fatal (6 / 29 849 files; the parse-rate gate at 99.5% still passes at 99.98%), but the family is now actionable rather than mysterious.

## Related

#2105 (the 325 → 6 fix), #2201 (its `SF_WEAK_REF_GAP` correction), #1882 (the +2 B opaque tail on the same block), #746/#747 (the original mis-attribution this closes out).

## Suggested Fix

Do **not** guess the field's semantics. Two safe steps:
1. Make the water-reference loop defensive — bail to the block-size boundary rather than issuing a `skip()` that provably exceeds it, so the block keeps its `NiNode` base and children instead of collapsing to `NiUnknown`.
2. Byte-audit the 10-byte run against nifly's `BSWeakReference` to determine whether it is a conditional per-entry field or a block-level one, using the clean/failing sibling pair above as the differential.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix


---

# #3532: LC-2026-08-27-D1-01: #2456's deferred-decision instrumentation now has its corpus answer (1 hit in 642,589 matrices), and its classifier cannot see the one case SVD cannot repair

Labels: bug, nif-parser, low, legacy-compat, tech-debt, nif

**From:** `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md` (LC-2026-08-27-D1-01) · base `969d81c8`

- **Severity**: LOW
- **Dimension**: 1 — coordinate-system / transform-model fidelity (Dimension 7's "Transform model" bullet is the sibling)
- **Location**: `crates/nif/src/rotation.rs:52-60` (the warning cap + its stated purpose), `:105-162` (`repair_rotation_svd_or_identity` + `sanitize_rotation`), specifically `:117` (`if nearest.determinant() < 0.0`)

## Description

`sanitize_rotation` carries deliberately-temporary instrumentation whose own doc states its purpose:

> "#2456 — this is diagnostic-only instrumentation to measure real corpus incidence before committing to the larger 'decompose into `NiTransform.scale`' fix; it changes no parsed geometry or transform output." (`rotation.rs:55-57`)

and

> "Neither branch folds the discarded factor into `NiTransform.scale` yet — that decomposition is deferred pending real-corpus incidence data." (`rotation.rs:141-143`)

That data now exists and it says the decomposition is not needed for any shipped Bethesda title. Two separate observations:

1. **Measured incidence is effectively zero.** Instrumenting `sanitize_rotation` and running it over 55,949 vanilla NIFs — `Oblivion - Meshes.bsa`, `Fallout - Meshes.bsa` (FO3), `Fallout - Meshes.bsa` (FNV), `Skyrim - Meshes0.bsa` + `Meshes1.bsa` — yields **642,589** `NiTransform` rotation matrices, of which **1** trips `is_degenerate_rotation` (the SVD branch) and **0** trip the `is_non_orthonormal` pass-through branch. The `diag(2, 0.5, 1)`-shaped "baked scale/shear" case the deferred fix was designed for does not occur in any of the four corpora.
2. **The classifier cannot distinguish a reflection from scale/shear, and silently changes orientation rather than losing magnitude.** A pure reflection (`diag(-1, 1, 1)`) is orthonormal — `is_non_orthonormal` returns `false` for it — but `is_degenerate_rotation` returns `true` (|det − 1| = 2), so it takes the SVD branch and logs the fixed text *"NiTransform.rotation is non-orthonormal (baked scale/shear, SVD-orthogonalized) — the singular value information is discarded"* (`rotation.rs:66-70`), which is factually wrong for it: a reflection has all singular values 1 and no scale/shear information to discard. Worse, `:117`'s `if nearest.determinant() < 0.0 { flip column 2 }` does not "repair" a reflection — it converts it into a **different orientation**. Verified by running the code: `diag(-1, 1, 1)` comes back as `diag(-1, 1, -1)`, i.e. a 180° rotation about Y, not an un-mirrored identity. So the eventual scale-decomposition fix would not address reflections at all, and the incidence data the warning gathers silently conflates the two classes.

## Evidence

Temporary counters added to `sanitize_rotation` and driven from a temporary `crates/nif/examples/_tmp_lc_rot.rs` (both reverted): `nifs=55949 matrices=642589 degenerate=1 nonortho_passthrough=0 clean_reflection=0`. Separately, a temporary `#[test]` in `crates/nif/src/rotation.rs` printed `degenerate=true non_ortho=false maxcol=1` / `repaired=[[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]]` / `det_after=1` for `diag(-1, 1, 1)`.

Re-verified at HEAD: the instrumentation, its "deferred pending real-corpus incidence data" doc, and the unconditional `determinant() < 0.0` column-2 flip are all still present.

## Impact

None on any shipped Bethesda title — this is why it is LOW, and why the reflection half is explicitly *not* filed as a content-mapping gap. Two costs remain: (a) a deferred design decision stays open with its blocking evidence already collectable in ten minutes, and every future audit that reads `rotation.rs:141-143` re-inherits "pending real-corpus incidence data" as an open question; (b) the reflection path is latent for non-Bethesda / mod content, which is live scope (issue #2383, "non-Bethesda titles"), and would fail in the most confusing possible way — a wrongly-*oriented* subtree reported in the log as a scale/shear problem.

## Related

#2456 (the instrumentation), #333 (the unit-quaternion guard downstream), #2383 (non-Bethesda titles). No existing issue covers the reflection classification.

## Suggested Fix

Record the measured incidence in `rotation.rs`'s doc (or close #2456 as "no vanilla incidence; decomposition not warranted") and drop or demote the rate-limited warning. If the instrumentation is kept, split the reflection case out: gate it on `det < 0` before the scale/shear wording, and say plainly in the message that the orientation — not just a magnitude — is being changed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other transform-sanitisation call sites and `#333`'s downstream unit-quaternion guard)
- [ ] **TESTS**: A regression test pins this specific fix (a `diag(-1, 1, 1)` case asserting the classified branch and the emitted message)


---

# #3543: SK-D4-01: the Deleted (0x20) tombstone is honoured only for placements — 9 DLC-deleted base records merge live on vanilla Skyrim

Labels: bug, medium, legacy-compat, game:skyrim, esm-plugin

**Source**: `docs/audits/AUDIT_SKYRIM_2026-08-30.md` — Dimension 4 (Multi-Master Load Order)
**Severity**: MEDIUM
**Location**: `crates/plugin/src/esm/cell/walkers.rs` (`RECORD_FLAG_DELETED`), `crates/plugin/src/esm/records/` (no test), `EsmIndex::merge_from`, doc at `crates/plugin/src/esm/cell/mod.rs`

## Description

`RECORD_FLAG_DELETED` (0x20) is tested at exactly one site in the whole crate —
`crates/plugin/src/esm/cell/walkers.rs`, inside the REFR/ACHR/ACRE placement walk. No
parser under `crates/plugin/src/esm/records/` tests bit `0x20`, so a **base** record that
a later plugin marks Deleted is merged by `EsmIndex::merge_from` under plain
last-write-wins and **replaces the master's live record with the DLC's tombstoned copy**.

## Evidence

Verified against current code (2026-08-30): `grep -rn RECORD_FLAG_DELETED crates/plugin/src`
returns the constant declaration plus **one** test at the REFR walk, and three doc mentions.
Zero record parsers consult it.

Measured on the shipped Skyrim SE DLC set (excluding NAVM, which has no consumer, and
REFR/ACHR, which the placement walker already skips):

| Plugin | Type | FormID (raw) | `data_size` | header flags |
|---|---|---|---|---|
| `Update.esm` | STAT | `0006CD7C` | 153 | `0x04010820` |
| `Dawnguard.esm` | STAT | `000BD6A5` | 198 | `0x00000020` |
| `Dawnguard.esm` | **NPC_** | `0007932F` | 220 | `0x00040020` |
| `Dawnguard.esm` | IDLE | `000FDC30` | 210 | `0x00000020` |
| `Dawnguard.esm` | IDLE | `000F6CBB` | 192 | `0x00000020` |
| `Dawnguard.esm` | SMQN | `000F2199` | 147 | `0x00000020` |
| `Dragonborn.esm` | **SPEL** | `0010E38C` | 307 | `0x00000020` |
| `Dragonborn.esm` | INFO | `000CEFBE` | 20 | `0x00000020` |
| `Dragonborn.esm` | EXPL | `000F3A8C` | 163 | `0x00000020` |

Every one carries a **non-empty payload** (20–307 bytes), so these are full override
records, not zeroed stubs the merge would harmlessly absorb. Every raw FormID has top byte
`0x00` — master index 0 = `Skyrim.esm` — so all nine are DLC overrides of base-game records
that the DLC then deletes. Correct behaviour: drop the base record from the merged index.
Actual behaviour: keep it, carrying the DLC's stale content.

The associated doc over-claims: `crates/plugin/src/esm/cell/mod.rs` states deleted records
"never appear in `over` at all" — true of REFRs, but read in a file named `cell/mod.rs` it
reads as though the tombstone story is complete. It is complete for placements only.

## Impact

Nine vanilla records merge live that should be removed, including `Dawnguard.esm`'s deleted
`NPC_ 0007932F` (still spawnable, still resolvable by the equip chain) and
`Dragonborn.esm`'s deleted `SPEL 0010E38C`. Scope on vanilla is small — which is why this is
MEDIUM, not HIGH — but the mechanism is general: a mod load order with real conflict
resolution hits it at far higher volume than vanilla does.

## Suggested Fix

One flag test in the record walk mirroring the placement-walk site, plus a removal signal
through `merge_from` analogous to `CellData::deleted_refs` (#2370). Correct the
`cell/mod.rs` doc to say the tombstone is honoured for placements only.

## Related

#2370 (`CellData::deleted_refs`), #1660 (REFR tombstone skip).

## Completeness Checks
- [ ] **SIBLING**: the same 0x20 test applied across every record parser, not just the one type that motivated the fix
- [ ] **LOCK_ORDER**: if a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: a regression test pins a DLC-deleted base record being dropped from the merged index (use one of the nine measured FormIDs)


---

