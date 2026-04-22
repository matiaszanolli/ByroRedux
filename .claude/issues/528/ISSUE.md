# FNV-CELL-2: Cloud texture load bypasses resolve_texture

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/528
- **Severity**: LOW
- **Dimension**: Cell loading
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`byroredux/src/scene.rs:185-223`

## Summary

Cloud layer 0 loads via raw `tex_provider.extract` + `load_dds` rather than `resolve_texture`. No duplication today (only one caller), but fragile once TOD crossfades add cloud layers 1..3.

Fix: replace 40 lines with `resolve_texture(ctx, &tex_provider, wthr.cloud_textures[0].as_deref())`.

Fix with: `/fix-issue 528`
