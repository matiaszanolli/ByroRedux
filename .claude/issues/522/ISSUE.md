# FNV-CELL-1: resolve_texture cache key divergence on textures\ prefix

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/522
- **Severity**: MEDIUM
- **Dimension**: Cell loading / memory
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

- `byroredux/src/asset_provider.rs:200-233`
- `crates/renderer/src/texture_registry.rs:249-292, 625-628`

## Summary

`TextureRegistry::normalize_path` does lowercase + slash-flip but not `textures\` prefix normalization. Calls with `landscape\foo.dds` and `textures\landscape\foo.dds` get different cache keys → same DDS extracted + uploaded twice → duplicated bindless slot + VRAM.

Fix: push prefix normalization into `normalize_path` so all three sites share one canonicalization.

Fix with: `/fix-issue 522`
