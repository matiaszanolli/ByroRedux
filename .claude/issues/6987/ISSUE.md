# #6987: TD8-2026-11-15-03: Four `cfg_attr(not(test), allow(dead_code))` items are production-dead and test-only

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6987
- **Labels**: low,terrain-exterior,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 8
- **Location**: `byroredux/src/cell_loader/lod_coverage.rs:70` (`find_full_detail_overlaps`); `byroredux/src/cell_loader/lod_bands.rs:592` (`select_authored_lod_quads`); `byroredux/src/cell_loader/tree_lod.rs:180,189` (`DtlTreeType`, `parse_treetypes_lst`)
- **Age**: `8e7624256` (lod_coverage), `0419437ab` (lod_bands), `510e7dc4d` (tree_lod), all 2026-11-08
- **Effort**: trivial (small for the tree_lod pair if wiring is chosen)
- **Labels**: tech-debt
- **Status**: NEW (the recipe's own "known" list; none was ever filed)
- **Description**: each is `pub(crate)`, carries `#[cfg_attr(not(test), allow(dead_code))]`, and has no production caller. `grep -rnw` at HEAD:
  - `find_full_detail_overlaps`: 4 hits in `lod_coverage.rs` tests (`:273-285`); production counts via `find_terrain_full_detail_overlaps` (the allow's own comment says so). Doc references: `lod_coverage.rs:19` and a comment at `streaming_helpers.rs:412`.
  - `select_authored_lod_quads`: a one-line wrapper over `select_authored_lod_quads_refining(.., -1, None)`. Callers are 8 tests in `lod_bands.rs` plus `legacy_lod_index_tests.rs:64,171`. Production calls `_refining` (the allow says so).
  - `parse_treetypes_lst` / `DtlTreeType`: callers are `tree_lod_tests.rs:443,449,484` (incl. the A6 corpus test). The struct doc says "nothing draws from them yet", so the FO3 `treetypes.lst` is parsed to a decoded-but-unexplained seven-float row that no draw path reads.
- **Impact**: three of the four allows exist only to silence a warning a `#[cfg(test)]` gate would turn into correct structure. Nothing is broken.
- **Related**: TD8-2026-10-31-01 (#6587) fixed the same pattern on `LootOutcome.stolen`.
- **Suggested Fix**: put `#[cfg(test)]` on `find_full_detail_overlaps` and `select_authored_lod_quads` (the latter's tests can keep calling it). For the treetypes pair either gate the parser under `#[cfg(test)]` with the A6 note kept, or open the follow-up that gives the seven floats a consumer. Drop the three `cfg_attr` lines.
- **Sites added since the skill snapshot (answer to the brief)**: the three `allow(dead_code)` sites added 2026-11-15 are `crates/renderer/src/vulkan/context/mod.rs:2026` (`hot_path_maps_are_fx_hashed`), `crates/renderer/src/vulkan/shadow_filter.rs:583` (`frame_images`) and `byroredux/src/render/skin_offsets_hasher_tests.rs:26` (`skin_offsets_is_fx_hashed`). All are never-called type-ascription guards inside test code (d84a67cd9 #6687, f5210e6e6 #6686) and the allow is correct. The four items above (2026-11-08) are the other recent additions. Other non-test sites checked: `LootSelection::Stack` (`inventory/loot.rs:136`, #4464, documented latent), `INHERIT_MAP` (`env_translate/mod.rs:97`, documented "parsed but unconsumed"), `LightHeader.count` (`buffers.rs:28`, GPU byte-copy), `QueryRead.guard` (`query.rs:26`, lock holder), the three `VF_*` constants in `bs_tri_shape.rs` (schema bits, reason given). All justified.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
