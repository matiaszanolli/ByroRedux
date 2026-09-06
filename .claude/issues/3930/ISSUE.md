# #3930: SF2-2026-09-05-D2-01: `SkinAttach` carries the authored bone names for 100% of the skins #3549 has to solve geometrically — parsed into `skin_attach_bones`, read by nothing

Filed from `docs/audits/AUDIT_STARFIELD_2026-09-05b.md` (SF2-2026-09-05-D2-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `high,game:starfield,legacy-compat,nif-parser,nif,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3930 --json state`.

---

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-05b.md` (SF2-2026-09-05-D2-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: HIGH
- **Dimension**: 2 — BSGeometry mesh extraction / skin chain
- **Location**: `crates/nif/src/blocks/extra_data.rs` (`NiExtraData::skin_attach_bones`),
  `crates/nif/src/import/mesh/skin.rs` (the `external_names` block),
  `crates/nif/src/import/mesh/skeleton.rs` (`solve_bone_names`,
  `resolve_external_bone_names`)
- **Status**: NEW — supersedes the premise of **#3549** (CLOSED 2026-08-30)
- **Description**:
  #3549 fixed "every Starfield skinned mesh has 100% unresolved bones — all SF
  actors and apparel render in bind pose" by adding a geometric solver: for a
  skin whose `BSSkin::Instance.bone_refs` are all NULL, fit the per-file bind
  offset against an externally-resolved skeleton and accept the names only on a
  unique fit, otherwise decline to the `Bone{i}` placeholder. Measured recovery
  was ~21% of clothes skins (~3,900 of ~19,500 bones); the rest correctly
  decline. The reasoning is recorded verbatim in `skin.rs`:

  > `// The identity is not in the file at all (its header string table holds`
  > `// only ExportScene, BSX, the mesh name and material paths), so`
  > `// resolve_node_name returned None …`

  That premise is false. The identity is in the file — not in the *header
  string table* (the comment is literally true about that), but in a
  `SkinAttach` extra-data block hanging off the very same `BSGeometry`, whose
  bone names are stored as **inline length-prefixed `NiString`s**, which is why
  they never appear in the header table. `crates/nif/src/blocks/mod.rs`
  dispatches `SkinAttach` to `NiExtraData::parse`, which decodes the list into
  `NiExtraData::skin_attach_bones: Option<Vec<String>>`. Nothing consumes it.
- **Evidence**:
  Per-shape structural walk (not scene-level co-occurrence): for each
  `BSGeometry`, follow its own `av.net.extra_data_refs` to a `SkinAttach`, and
  its own `skin_instance_ref` → `bone_data_ref` to the authoritative bone
  count. Swept over `Meshes01` + `MeshesPatch` + `FaceMeshes` +
  `ShatteredSpace - Main01`:

  ```
  BSGeometry shapes with a skin instance      = 21,222
    skin has ALL-NULL bone_refs               = 18,990   (89.5%)
      shape's OWN extra_data has SkinAttach   = 18,990   (100.0%)
        names.len() == BoneData bone count    = 18,990   (100.0%)
        count mismatch                        =      0
    skin bone_refs resolve (control group)    =  2,232
      OWN SkinAttach agrees in ORDER + name   =      0
      OWN SkinAttach disagrees                =  1,640
  ```

  The control group is what makes this conclusive rather than coincidental.
  Where `bone_refs` *do* resolve, the shape's `SkinAttach` entries are **empty
  strings** — the names live in the node refs instead. The two mechanisms are
  complementary alternatives, exactly as a "0 → use the sibling channel"
  encoding would be:

  ```
  ORDER_DISAGREE meshes\actors\minibota\mesh\minibota_security\minibota_security.nif
      attach=["", "COM", "", ""]  resolved=["C_Chassis","C_Body","C_Axle","C_Base"]
  ```

  And the recovered names on the all-null population are unambiguous Starfield
  skeleton bones, i.e. precisely what the solver is trying to reconstruct:

  ```
  meshes\clothes\spacesuit_ucpilot_01\spacesuit_ucpilot_lowerbody_01_f.nif   n=21
      ["R_Foot","R_Calf","R_Toe","C_Hips","C_Spine","C_Spine1","R_Butt","L_Butt", …]
  meshes\clothes\spacesuit_starborn_01\spacesuit_starborn_hunter_plates_f.nif n=43
      ["R_Clavicle","R_Deltoid","R_Biceps_Twist1","C_Chest","R_Biceps","R_Elbow", …]
  meshes\actors\human_crowd\mesh\female\hairs\messy_business_f_crowd.nif      n=1
      ["C_Head"]
  ```

  Repo-wide consumer check — `skin_attach_bones` is written by the parser and
  read only by a dispatch test:

  ```
  crates/nif/src/blocks/extra_data.rs        (declaration + 4 assignment sites)
  crates/nif/src/blocks/dispatch_tests/starfield.rs:281,283
  crates/nif/src/import/mesh/tangent_convention_tests.rs:517   (struct-literal `None`)
  ```

  No hit in `crates/nif/src/import/mesh/skin.rs`, `skeleton.rs`, or anywhere in
  `byroredux/src/`.
- **Impact**:
  ~79% of Starfield skinned content — the share `solve_bone_names` correctly
  declines on rather than guessing — stays in bind pose, when the authored bone
  names for 100% of it are already sitting parsed in memory. Blast radius is
  every NPC body, every head (all 1,282 `FaceMeshes` NIFs are in the affected
  population), and every apparel/spacesuit piece: 18,990 of 21,222 skinned
  shapes in the four sampled archives. This is the single largest remaining
  correctness gap on the Starfield content path, and unlike the geometric
  solver it needs no external skeleton resolution, no tolerance tuning, and no
  decline path — the data is exact and count-checked.
- **Related**: #3549 (CLOSED — the solver this supersedes as the primary
  source); #708 / NIF-D5-02 (added the `SkinAttach` parser); `SF2-…-D2-02`
  below (the `BoneTranslations` sibling, same class).
- **Suggested Fix**:
  In `crates/nif/src/import/mesh/skin.rs`, before the `external_names`
  geometric fallback, look up the owning shape's own `SkinAttach` via
  `extra_data_refs` and use its list as the primary name source. Resolve
  **per entry**, not wholesale — `minibota_security.nif` proves a single
  `SkinAttach` can mix authored and blank entries — giving the chain
  `SkinAttach[i]` (if non-empty) → `resolve_node_name(bone_refs[i])` →
  `solve_bone_names` → `Bone{i}`. Keep the geometric solver as the last
  resort it already is; this only moves it behind the authored data. Assert
  the count agreement (`names.len() == bone_data.bones.len()`) and decline the
  whole list on a mismatch, mirroring the existing decline discipline.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
