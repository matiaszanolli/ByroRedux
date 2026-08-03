# TD3-204: docs/engine/renderer.md quotes stale GpuInstance (112 B) / GpuMaterial (300 B) sizes

**Severity**: MEDIUM
**Dimension**: 3 (Stale Documentation & Comments)
**Location**: `docs/engine/renderer.md:129-130`, `:499`
**Labels**: medium, renderer, tech-debt, documentation
**Source**: `docs/audits/AUDIT_TECH-DEBT_2026-08-03.md`

## Description
`GpuInstance` grew 112→128 B via commit `4ddf754a` (#2219, reconstructing skinned RT
hit normals from deformed geometry) and `GpuMaterial` grew 300→348 B via an earlier
commit (`1d94eb24`). `docs/engine/shader-pipeline.md`, every `audit-*/SKILL.md`, and
`_audit-common.md` were correctly updated at the time — `renderer.md` was not.
#2219's own commit patched a *different* GpuInstance mention 30 lines later in the
same file but skipped these two, and never touched `GpuMaterial` here at all even
though the sibling docs got that update via an unrelated commit that also never
touched `renderer.md`.

## Evidence
`docs/engine/renderer.md:129-130` and `:499` (112 B / 300 B) vs.
`crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`
(`gpu_instance_is_128_bytes_std430_compatible`, `gpu_material_size_is_348_bytes`
in `material.rs:1272`) — both live at 128 B / 348 B.

## Impact
`#[repr(C)]` GPU struct size/layout drift is a HIGH-minimum category per severity
rules when it's a live code drift; here the code is correct and only the doc is
stale, which keeps this at MEDIUM. Still misleads any reader of `renderer.md`
specifically, which is otherwise treated as authoritative.

## Related
Same failure class as the already-fixed TD3-NEW-01 (Vertex byte size) and TD3-201
(GpuLight fields) — third recurrence of this exact doc, different fields, one
cycle later.

## Suggested Fix
Update both call-sites to 128 B / 348 B, matching `shader-pipeline.md`'s wording.

## Age / Effort
`GpuInstance` drift is 1 day old (#2219 landed today); `GpuMaterial` drift predates
this window. Effort: trivial.
