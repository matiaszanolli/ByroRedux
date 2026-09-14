# #4386 — TD8-007: `_`-prefixed parameters that survived refactors in production functions

**Labels**: low, renderer, nif-parser, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4386

- **Severity**: LOW · **Dimension**: 8
- **Location**: `byroredux/src/asset_provider/material/provider.rs:292` (`register_starfield_cdb_probe(_info)`), `crates/nif/src/blocks/shader.rs:957` (`parse_skyrim(_bsver)`, private, 1 caller), `crates/renderer/src/vulkan/frame_upscaler.rs:1052` (`destroy_device_objects(_device)`) · **Status**: NEW · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: Because the CDB probe discards its `_info`, `sf_cdb_cache` stores a whole `Option<CdbHeaderInfo>` where a bool would do. The table-bound `prim_*` signatures, the `_game` seam in `light_anim.rs` and the fixed system/trait signatures were checked and are justified.
- **Suggested Fix**: Drop the parameters (and shrink the CDB cache value to `bool`).

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
