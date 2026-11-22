# #7165 — PAR-D4-2026-11-21-02: the #7140 ledger test can be satisfied by unrelated mentions of a field name

Labels: low,import-pipeline,bug,test-gap

- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `byroredux/src/asset_provider/tests/bgsm_merge.rs` `every_parsed_sidecar_field_is_merged_or_in_the_ledger`
- **Status**: NEW (introduced by #7140)
- **Description**: the needles `.field` and `` `field `` are searched across the whole production text of three files, and those needles match:
  - any receiver, including `ImportedMaterial` writes (`material.two_sided`, `material.specular_color`, `material.textures.base_color`);
  - fields of the other parsed structs (`BgemFile::emittance_color` is masked by `bgsm.emittance_color`);
  - mentions in comments.

  As a result, deleting the real read of `translucency_subsurface_color`, `translucency_transmissive_scale`, `model_space_normals`, `specular_color`, `alpha_test`, `two_sided` or BGEM `base_color`, `emittance_color` or `environment_mapping*` keeps the test green. Lines of the form `material.f = bgsm.f;` are genuinely guarded.
- **Evidence**: `/tmp/audit/parsers/misc/ledger_probe.py` (a re-implementation of the scan).
- **Suggested Fix**: key the needle by receiver per struct (`bgsm.`/`.base.`/`leaf.` for BGSM and the base, `bgem.` for BGEM), strip `//` comments before scanning, restrict ledger needles to the ledger block, and list accessor-consumed fields (`env_mapping_enabled`) explicitly.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
