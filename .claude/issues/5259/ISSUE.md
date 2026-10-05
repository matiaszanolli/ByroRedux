# #5259: NIF-D3-2026-10-05-01: the `.nif`-only corpus rule is still hand-rolled in two gated corpus tests and ~12 examples after #4629's "last five" sweep — one of them added in-window

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5259
- **Labels**: low,nif-parser,nif,bug,test-gap,game:fo4,game:fo76,game:skyrim
- **Source**: `docs/audits/AUDIT_NIF_2026-10-05.md` (NIF-D3-2026-10-05-01)

_From `docs/audits/AUDIT_NIF_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW (test-gap)
- **Dimension**: Block Dispatch Coverage (baseline harness)
- **Game Affected**: FO4 / FO76 / Skyrim SE distant-LOD `.bto` / `.btr` content
- **Location**:
  - `crates/nif/tests/translation_completeness.rs:380`, `:998`;
  - examples: `r6_bone_count_survey.rs:42,49`, `d5_listba2.rs:17`, `dump_transforms.rs:135,146`, `havok_blob_recon.rs:92`, `property_controller_census.rs:37`, `sf_matpath_dump.rs:35`, `sf_slot_census.rs:63,70`, `ragdoll_components.rs:28`, `ambient_light_census.rs:45,52`, `sf_root_material_sample.rs:45`, `emissive_census.rs:38,45`.
- **Status**: NEW. #4629 (closed) listed only five examples. #4154 (closed) established `corpus::is_nif_entry` as the shared rule.
- **Description**: the corpus definition is meant to live once, in `corpus.rs` (`NIF_ENTRY_EXTENSIONS` includes `.bto`/`.btr`); a second private copy is the skill's named regression. Two `#[ignore]` corpus tests still filter with `.ends_with(".nif")`:
  - `cross_game_translation_completeness` sampling;
  - `dark_texture_role_population_matches_the_documented_census`, whose doc calls the census "complete" over "every mesh archive of every installed game".
  `sf_matpath_dump.rs` was added on 2026-09-30 (224a19372), after the rule existed. The deliberate scopes at `parse_real_nifs.rs:698` (torch folders) and `:926` (`.nif` ∪ `.kf` census) are excluded from this list.
- **Impact**: the translation-completeness sample and the dark-role census silently exclude every renamed-NIF LOD mesh (22k+ on FO76 alone). The census tools under-report on LOD-heavy archives, which is the #4154/#4629 failure shape. No parser defect.
- **Related**: #4629, #4154.
- **Suggested Fix**: route both test sites through `byroredux_nif::corpus::is_nif_entry` and sweep the examples. Optionally add a source scan in `corpus.rs` tests that rejects `ends_with(".nif")` under `tests/` and `examples/`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
