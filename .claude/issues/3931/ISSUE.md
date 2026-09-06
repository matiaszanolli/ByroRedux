# #3931: SF2-2026-09-05-D2-02: `BoneTranslations` is decoded on every instance and consumed by nothing — the same drop as D2-01, one order of magnitude smaller

Filed from `docs/audits/AUDIT_STARFIELD_2026-09-05b.md` (SF2-2026-09-05-D2-02) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `medium,game:starfield,legacy-compat,nif-parser,nif,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3931 --json state`.

---

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-05b.md` (SF2-2026-09-05-D2-02), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: 2 — BSGeometry mesh extraction / skin chain
- **Location**: `crates/nif/src/blocks/extra_data.rs`
  (`NiExtraData::bone_translations`)
- **Status**: NEW
- **Description**:
  `BoneTranslations` is dispatched alongside `SkinAttach` to
  `NiExtraData::parse` and its payload — `(bone_name, [f32; 3])` pairs
  supplying per-bone offset deltas for the skeleton at a given LOD, sourced
  from `nifly::BoneTranslations::Sync` — is fully decoded into
  `NiExtraData::bone_translations`. As with `skin_attach_bones`, the field has
  no consumer outside the block parser and its own dispatch test. Every
  instance that ships in vanilla Starfield carries a non-empty payload, so this
  is not a dormant field waiting for content that does not exist.
- **Evidence**:
  Swept over the same four archives:
  ```
  BoneTranslations blocks           = 256
    with a decoded payload          = 256   (100%)
  ```
  Corpus-wide the histogram counts 281 instances across all 13 archives.
  Consumer grep over `crates/`, `byroredux/`, `tools/` returns only
  `crates/nif/src/blocks/extra_data.rs` (declaration + assignment),
  `crates/nif/src/blocks/dispatch_tests/starfield.rs:294,314,316`, and a
  struct-literal `None` in `tangent_convention_tests.rs:518`.
- **Impact**:
  Per-bone LOD offset deltas are dropped, so a skinned mesh at a reduced
  skeleton LOD is posed from the unadjusted bind data. Far narrower than
  D2-01 (281 instances vs 22,378 `SkinAttach`), and it only manifests at LOD
  boundaries rather than at LOD 0, which is why it is MEDIUM rather than HIGH:
  translatable data silently dropped, with a visible but bounded consequence.
  Worth fixing in the same change as D2-01, since both hang off the same
  `extra_data_refs` walk.
- **Related**: `SF2-…-D2-01`; #708 (added both parsers in one commit).
- **Suggested Fix**:
  Carry the pairs onto `ImportedSkin` alongside the bone list, keyed by name so
  they survive the `SkinAttach`/`bone_refs` name resolution above, and apply
  them when a non-zero bone-LOD level is selected. If no LOD selection exists
  yet, the honest interim is to record the field as deliberately deferred with
  a dated comment rather than leave it looking wired.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
