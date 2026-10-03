# #5177: EXT-D3-2026-10-02-05: Doc rot from #4903 / #4907 / #4906 (bundle)

**Labels**: low,terrain-exterior,documentation,doc-rot
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW (doc rot / test gap)
- **Dimension**: Ground-cover pipeline (plus one Dim 2 test comment)
- **Location**: see each item
- **Status**: NEW
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description**:
  1. **`docs/engine/exal-groundcover.md`, three stale sections.** None of the three fix commits touched the spec doc.
     - §3 (`:151-153`) still describes `affinity(splat)` as the 8 weights "dotted with" `cover_affinity`. It is now an ordered mix from the BTXT base.
     - §12.3 (`:1195`) still says the blade blends toward "the splat-weighted average of the cell's painted layer diffuse textures". It now uses BTXT base plus `byroTerrainSplatAlbedo`.
     - §12.12 (`:1624-1626`) states accept probability `density × …` and pure climate selection. It omits the `density/share` bake and the placeable-only weighting.
  2. **`groundcover_blade.frag:136-147`** still says the blend is "a weighted average … not `mix` against a base texture: the blade's base has no BTXT base layer of its own". That is the premise #4907 removed, and the code directly below contradicts it.
  3. **Three doc comments repeat the false premise #4903 removed**: `DEFAULT_COVER_AFFINITY` (`crates/core/src/ecs/components/groundcover.rs:107-113`), `groundcover_translate.rs:71-73`, and `shader_constants_data.rs:232-236`.
     - They say the scatter shader uses the default for unpainted ground because the base "has no LTEX record".
     - `GROUNDCOVER_DEFAULT_AFFINITY` is still emitted to GLSL but no shader reads it.
  4. **Moot rationale.** `terrain.rs:1084-1088` and `components.rs:471-473` say unused slots must hold the default "or an unused layer reads as a vegetation hole". Under the ordered mix a zero-weight lane cannot affect the result.
  5. **Weak #4903 guard.** The "host-side half" of `groundcover_affinity_composes_the_base_in_diffuse_loop_order` exercises a closure defined inside the test, not the shader. The source half counts 8 `mix` calls but not lane pairing: `affinity0.y` paired with `splat0.x` would pass.
  6. **Wrong direction in a #4905 test comment.** `terrain_splat_tests.rs:190` labels SW row 0 "top edge (faces the cell above)". Row 0 is the south border: row 16 is north, as the same test's `:193` says. The assertion itself is correct.
- **Suggested Fix**:
  - Rewrite the cited text.
  - Drop the dead GLSL constant export, or document it as Rust-only.
  - Pin lane pairing in the #4903 guard.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it
