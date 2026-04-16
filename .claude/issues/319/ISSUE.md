# Issue #319 — R34-01/R34-02: Stale renderer doc comments

- **Severity**: LOW | **Source**: AUDIT_RENDERER_2026-04-14.md | **URL**: https://github.com/matiaszanolli/ByroRedux/issues/319

R34-01: `vertex.rs:11-13, 23-24` references push-constant `model` and `bone_offset` — those are SSBO fields now (#294).
R34-02: `helpers.rs:48-54` says normal is `RGBA16_SNORM` — actual is `RG16_SNORM` (octahedral, #275); also doc the implicit 65534-instance ceiling on R16_UINT mesh_id.
