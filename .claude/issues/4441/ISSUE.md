# #4441: SF-2026-09-16-D8-01: #4284 corrected one of three "classifier arm is only a future backstop" statements; the boundary's own doc and the corrected block still misstate the live NaN producers

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4441
- **Labels**: low,nifal,documentation,doc-rot,game:starfield,legacy-compat
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW
- **Dimension**: 8
- **Location**:
  - `byroredux/src/material_translate.rs:528-537`
  - `crates/core/src/ecs/components/material.rs:1307, 1334-1337`
- **Status**: NEW (a missed sibling of CLOSED #4284)
- **Description**: The inaccurate statements:
  - **Boundary doc**: `translate_material`'s contract doc still says
    `resolve_pbr`'s "classifier arm is a sentinel-backstop (only fires when
    the override is `NaN`, i.e. for future non-NIF paths). BGSM/BGEM content
    also arrives pre-classified as `Some`." This is exactly the text #4284
    fixed in `Material::resolve_pbr`.
  - **The block #4284 corrected**: `material.rs:1307` still says "For
    **BGSM/BGEM** content the authored scalars also arrive as `Some`."
  - **Inline comment**: `material.rs:1334-1337` says the backstop "is
    unreachable for every pre-classified current producer (both NIF import …
    and BGSM/BGEM leave metalness/roughness non-NaN)".

  All three are false for BGEM. `merge_bgem_arm` deliberately leaves both
  overrides `None` (`merge.rs:993-995`: "metalness and roughness are left as
  NaN sentinels so resolve_pbr runs the keyword classifier"), and the stub it
  merges into has no classifier signal. The claims are also false for every
  Starfield stub (#2707).
- **Evidence**: The quoted text above.
- **Impact**: The boundary's own contract still describes the live arm (the
  majority Starfield path, and every FO4 / Starfield BGEM) as future-proofing.
  That is the same deletion hazard #4284 was filed for.
- **Suggested Fix**: Apply #4284's correction to `translate_material`'s doc
  and drop "BGEM" from the two `material.rs` sentences. Consider a test that
  asserts a BGEM-merged stub reaches `translate_material` with NaN scalars.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
