# #5281: NIFAL-D1-2026-10-05-02: #4912 left two names for one texture-only lowering: `translate_texture_only_material_with_authored_msn` is a pure forward to a private `…_with_authored_msn_and_clamp` with the identical signature

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5281
- **Labels**: low,nifal,tech-debt,bug
- **Source**: `docs/audits/AUDIT_NIFAL_2026-10-05.md` (NIFAL-D1-2026-10-05-02)

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW · **Dimension**: Material · **Tier Violated**: none (tech-debt) · **Game Affected**: all (terrain/LOD)
- **Location**: `byroredux/src/material_translate.rs:1038-1067`
- **Status**: NEW (introduced by `235a90ba2`)
- **Description**: `translate_texture_only_material_with_authored_msn(texture_path, model_space_normals, texture_clamp_mode)` does
  nothing except call `translate_texture_only_material_with_authored_msn_and_clamp` with the same three arguments. The texture-only
  boundary now spans `translate_texture_only_material`, `_with_clamp`, `_with_authored_msn` and the private `_and_clamp`, plus
  `_inner`. Every public name must also be kept in `every_exterior_spawner_inserts_a_boundary_material`'s `boundary_fns` needle
  list (`:2516-2525`), which grew by one in the same commit.
- **Impact**: Maintenance only. A further variant would grow the needle list again, and a missed needle fails the guard closed.
- **Suggested Fix**: Move the body into `translate_texture_only_material_with_authored_msn` and delete `_and_clamp`, with
  `_with_clamp` calling it with `false`.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
