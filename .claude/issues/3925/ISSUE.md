# #3925: OBL-2026-09-05-D6-01: Oblivion loses 730 of 9,612 vanilla meshes to `d49cd88b`'s skin-partition reservation, and 4,693 blocks with them

Filed from `docs/audits/AUDIT_OBLIVION_2026-09-05.md` (OBL-2026-09-05-D6-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `critical,game:oblivion,legacy-compat,nif-parser,nif,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3925 --json state`.

---

**Source**: `docs/audits/AUDIT_OBLIVION_2026-09-05.md` (OBL-2026-09-05-D6-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: CRITICAL
- **Dimension**: 6 — Real-Data Validation (with Dimension 1 mechanism)
- **Location**: `crates/nif/src/blocks/skin.rs` — the `num_strips > 0` branch of `NiSkinPartition::parse` (`stream.allocate_vec_sized::<[u16; 3]>(num_triangles as u32)?`), introduced 2026-09-03 in `d49cd88b`. Failure surfaces through `Stream::check_alloc` (`crates/nif/src/stream.rs`, the `bytes > remaining` arm) and is amplified by the sizeless-recovery path in `crates/nif/src/lib.rs` (`parse_nif`'s `Err` arm, the `truncated = true; break` fall-through).
- **Status**: NEW **for the Oblivion measurement**. Root cause is **Existing (cross-audit): PERF-D6-NEW-01** in `docs/audits/AUDIT_NIF_2026-09-04.md`, re-raised today by `/audit-fo3`. Also a **Regression of the fix for `#3691`** (CLOSED). Do not double-file the root cause — file the Oblivion consequence against the same issue, or add this measurement to it.
- **Description**: `allocate_vec_sized::<[u16; 3]>(n)` bounds `n * 6` bytes against the stream's remaining bytes. In the strip branch those triangles are *generated* by `strip::destrip` from u16 index arrays that cost ~2 B per emitted triangle, so the bound over-demands by ~3×. On Oblivion, where `NifHeader.block_sizes` is empty, the resulting `Err` has no `block_size` recovery: `parse_nif` sets `truncated = true` and **discards every subsequent block**.
- **Evidence** (all measured today, release build, all nine archives):

  Per-archive, HEAD:

  | Archive | NIFs | clean | truncated | stopped at `NiSkinPartition` |
  |---|---|---|---|---|
  | `Oblivion - Meshes.bsa` | 8,032 | 7,454 | 578 | 504 |
  | `DLCShiveringIsles - Meshes.bsa` | 1,438 | 1,302 | 136 | 118 |
  | `Knights.bsa` | 75 | 60 | 15 | 15 |
  | `DLCHorseArmor.bsa` | 4 | 3 | 1 | 1 |
  | 5 remaining DLC archives | 63 | 63 | 0 | 0 |
  | **Total** | **9,612** | **8,882 (92.41 %)** | **730** | **638** |

  In `Oblivion - Meshes.bsa`, **504 of 509** parse-stopping blocks are
  `NiSkinPartition` (the other 5 are `NiNode`, downstream cascade). All **505**
  `check_alloc` rejections request a byte count that is an **exact multiple of
  6** — `size_of::<[u16; 3]>()` — e.g. `NIF requested 468-byte read at position
  56258, only 377 bytes remaining` (78 triangles × 6 B demanded; the strip
  payload actually costs ≈ 156 B, well inside the 377 available).

  **Causality proof.** Single-line patch, strip branch only:
  `allocate_vec_sized::<[u16; 3]>(n)` → `allocate_vec_min_bytes::<[u16; 3]>(n, 2)`.
  Re-measured all nine archives: **9,612 / 9,612 clean, 0 truncated, 0
  `NiUnknown`**, and the per-block histogram matches
  `crates/nif/tests/data/per_block_baselines/oblivion.tsv` exactly on every
  affected row (`NiSkinPartition 1596 0`, `NiNode 25244 0`, `NiSkinData 1596 0`,
  `NiSkinInstance 1596 0`). Patch reverted; tree clean.

  **Block loss in `Oblivion - Meshes.bsa` (HEAD vs patched), 4,693 total:**

  | Lost | Type | HEAD parsed / unknown | Fixed parsed |
  |---:|---|---|---:|
  | 3,044 | `NiNode` | 22,200 / 464 | 25,244 |
  | 580 | `NiSkinPartition` | 1,016 / 74 | 1,596 |
  | 231 | `NiExtraData` | 52,101 / 0 | 52,332 |
  | 226 | `bhkRigidBody` | 8,504 / 0 | 8,730 |
  | 226 | `bhkCollisionObject` | 8,504 / 0 | 8,730 |
  | 220 | `bhkBoxShape` | 1,235 / 0 | 1,455 |
  | 141 | `bhkLimitedHingeConstraint` | 451 / 0 | 592 |
  | 6 | `bhkConvexVerticesShape` | 2,085 / 0 | 2,091 |
  | 19 | `NiTriShape` / `NiTriShapeData` / `NiTriStripsData` / `NiMaterialProperty` / `NiSkinData` / `NiSkinInstance` / `NiTransformController` / `NiTexturingProperty` / `NiSourceTexture` / `NiAmbientLight` | — | — |

  **Named vanilla casualties** (`nif_stats` truncated-file examples, and direct
  `recovery_trace` runs): `meshes\creatures\troll\troll.nif`,
  `meshes\creatures\goblin\shamanchest.nif` (39 blocks dropped),
  `meshes\creatures\goblin\handrberserker.nif` (15),
  `meshes\armor\daedric\m\cuirass.nif`, `meshes\armor\elven\m\greaves.nif` (10),
  `meshes\armor\fur\f\helmet.nif` (5),
  `meshes\armor\townguardcho\m\cuirass_gnd.nif` (12),
  `meshes\clothes\middleclass\04\m\shirt_gnd.nif` (18),
  `meshes\clothes\robelcgrey\m\robelcgreym_gnd.nif` (12),
  `meshes\clothes\robemcblack\m\robemcblack_gnd.nif` (12),
  `meshes\clothes\lowerclass\{08,12,15}\f\shirt.nif`,
  `meshes\clothes\amulet\{amuletgold,thornblademedallion,amuletjadejeweled}.nif`,
  `meshes\oblivion\clutter\containers\clawstandcontainer.nif`.
  The population is dominated by worn ARMO/CLOT meshes, their `_gnd` ground
  models, and creature bodies — i.e. the equipment/outfit rendering surface.
- **Impact**:
  - **Oblivion-specific amplification.** 580 `NiSkinPartition` blocks actually
    fail; **4,693** blocks are lost — an **8.1×** amplification that exists
    only because Oblivion ships no `block_sizes` table. FO3/FNV/Skyrim lose one
    block per failure and stay 100 % clean; Oblivion loses the file's tail.
  - **Physics.** 24 % of Oblivion's `bhkLimitedHingeConstraint` population and
    ~2.6 % of its rigid bodies vanish. The PHYSAL ragdoll articulation for the
    affected creatures/actors is built from a truncated constraint chain — a
    silent behavioural regression with no parse-level error.
  - **Rendering.** 3,044 `NiNode` blocks — whole scene-graph subtrees on
    equipment meshes — never reach `import_nif_scene`.
  - **Silent.** `MIN_RECOVERABLE_RATE = 1.0` gates *recoverable*, not *clean*,
    and stays green. The one gate that would go red
    (`crates/nif/tests/per_block_baselines.rs`) is `#[ignore]`d, needs game
    data, and has no CI runner — so the regression shipped invisibly.
  - **Cross-game blast radius confirmed.** The FO3 finding's severity should be
    escalated on this evidence: it is not a latent bound-tightening nit, it is
    a live 7.6-point content-loss regression on a shipped title.
- **Related**: PERF-D6-NEW-01 / PERF-D6-NEW-02 (`docs/audits/AUDIT_NIF_2026-09-04.md`); `#3691` (CLOSED, whose fix this is); `#2523` (the `allocate_vec_sized` / `allocate_vec_min_bytes` split this violates); `#1549` (de-strip); `#324` (the sizeless recovery path this cascades through); `ROADMAP.md:605` (documents 100 % / 8,032 of 8,032, now wrong).
- **Suggested Fix**: `stream.allocate_vec_min_bytes::<[u16; 3]>(num_triangles as u32, 2)?` — 2 B is the honest per-triangle minimum for strip-derived faces (verified: restores 9,612 / 9,612 and reproduces the checked-in baselines). Add a regression test whose partition authors a strip of ≥ 5 indices with < 3 × `len` bytes trailing. Separately, add a **clean**-rate floor to `run_game` in `crates/nif/tests/parse_real_nifs.rs` so a clean-rate slide cannot hide behind a green recoverable gate again.

---

### MEDIUM

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
