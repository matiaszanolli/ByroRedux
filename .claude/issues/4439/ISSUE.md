# #4439: SF-2026-09-16-D6-01: The #1510 "content-hash path" premise has no vanilla evidence — every suffix-less Starfield material reference is the degenerate string `Materials\`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4439
- **Labels**: low,nif-parser,nif,documentation,doc-rot,game:starfield,legacy-compat
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW
- **Dimension**: 6
- **Location**:
  - `crates/nif/src/blocks/shader/lighting.rs:579-588`
  - `crates/nif/src/blocks/shader/effect.rs:160-168`
  - `crates/nif/src/blocks/shader_tests/starfield.rs:274-313`
  - `crates/nif/src/import/mesh/material_path.rs:10-17`
- **Status**: NEW
- **Description**: The stub discriminator (`!name.is_empty()` for
  `bsver >= STARFIELD`) is justified in three places by the claim that
  "Starfield material references are content-hash paths with NO suffix
  (`<hash>\<hash>`)". The tests use the synthetic name
  `"8f3a91c4\\b27e5d06"`. The census found no such name.

  The gate itself is still correct: a non-empty name on vanilla content always
  means a stub. The premise is false, though, and it hides a downstream fact.
  The suffix-less stubs that do exist get `material_path = None`, because
  `material_path_from_name` keeps the suffix gate. `merge_external_material`
  then returns `Unresolved` for them, so they are the only Starfield stubs
  that never reach the CDB PBR route.
- **Evidence**: The census covered 120,543 NIFs and 480,861 non-empty-name
  shader stubs:

  | Name kind | Count |
  |---|---:|
  | `.mat` | 478,691 |
  | `.bgsm` | 1,679 |
  | `.bgem` | 104 |
  | suffix-less | **387** |

  Every suffix-less name is `"Materials\\"` (384) or `"\\Materials"` (3):
  editor markers, conveyor belts, pedestals and similar
  (`meshes\markers\editormarkers\editormarkerrocksmall.nif`, …). There are
  zero hex-hash paths.
- **Impact**: The comments are wrong, which misdirects Phase 2: a CDB lookup
  cannot key a hash-path name that does not exist. 387 shapes (0.08%) render
  on the legacy path with NIF-stub defaults, and no decision has been recorded
  on whether that is intended.
- **Suggested Fix**: Correct the three comments and the test fixture name to
  the measured `Materials\` form. Decide explicitly, with a test, whether
  directory-only references should take the CDB PBR fallback or stay
  `Unresolved`.

**CRC32 note**: Nothing changed since `AUDIT_STARFIELD_2026-09-11.md`. See the
table below.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **TESTS**: A regression test pins this specific fix
