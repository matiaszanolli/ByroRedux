# Issue #318 — R6-01: ui.vert GpuInstance struct drift (missing `flags`)

- **Severity**: LOW | **Source**: AUDIT_RENDERER_2026-04-14.md | **URL**: https://github.com/matiaszanolli/ByroRedux/issues/318

`ui.vert:11-31` declares slot at offset 152 as `_pad0; _pad1;`; triangle.{vert,frag} and Rust `GpuInstance` declare `flags; _pad1`. Same size (160 B) so no corruption, but breaks the Shader Struct Sync invariant. Rename `_pad0` → `flags`. Long-term: extract shared `gpu_instance.glsl` include.
