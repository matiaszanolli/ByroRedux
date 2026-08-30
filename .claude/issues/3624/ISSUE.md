# #3624 — REN-2026-08-30-D19-04: `parallax_alpha_height_bit_is_masked_and_honoured_by_every_reader` cannot enforce the "every reader" it is named for

**Labels**: `low,renderer,shaders,test-gap,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3624 --json state`.

---

- **Severity**: LOW
- **Dimension**: Tangent-space & normal maps
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:2012-2067`
- **Status**: NEW
- **Description**: The test's doc comment states the invariant in the strongest terms
  (*"**every** reader must mask it before using the value as a bindless index:
  `textures[0x8000000N]` is a wildly out-of-bounds descriptor read"*), but the test
  is a three-file whitelist (`material_sampling.glsl`, `ray_hit.glsl`,
  `triangle.frag`) and its per-file assertion is
  `src.contains("& ~PARALLAX_ALPHA_HEIGHT_BIT")` — an *at-least-once* substring check,
  not a check that every read site masks. A fourth shader added later that reads
  `mat.parallaxMapIndex` raw is not in the list and is not caught; a fourth *unmasked*
  read added to one of the three listed files also passes, because the file already
  contains one masked read elsewhere.
- **Evidence**: The invariant currently *does* hold — a repo-wide grep for
  `parallaxMapIndex` across `crates/renderer/shaders/` returns exactly four read sites
  (`triangle.frag:228`, `:238`, `:1569`; `ray_hit.glsl:296`, `:298`) plus the struct
  declaration in `include/bindings.glsl:125`, and all of them mask. So this is a
  guard-strength gap, not a live defect.
- **Impact**: The one mechanism protecting against an out-of-bounds bindless
  descriptor read cannot actually fail on the shape of mistake it exists to catch.
  Given `#3530` is three days old and the bit will accrue readers (`water.frag` and
  the RT hit shaders are the obvious next ones), the whitelist will silently go stale.
- **Suggested Fix**: Enumerate `crates/renderer/shaders/**/*.{frag,vert,comp,glsl}`
  at test time, and for every file containing `parallaxMapIndex` assert that each
  occurrence is either the `include/bindings.glsl` declaration or is immediately
  followed by `& ~PARALLAX_ALPHA_HEIGHT_BIT` / `& PARALLAX_ALPHA_HEIGHT_BIT`. The
  sibling `NORMAL_ALPHA_SPEC_BIT` (same value, same hazard) deserves the same
  treatment in the same pass.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D19-04

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
