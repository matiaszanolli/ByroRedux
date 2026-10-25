# TD8-2026-10-25-03: five `_`-prefixed parameters that survived refactors (delete, do not rename)

Labels: low, nif-parser, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6396
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Effort**: small · **Status**: NEW (sibling of closed #4386 / #6192 `_receiver_kind`)
- **Location** (each param unused in the body; all callers inside the same crate):
  - `crates/nif/src/import/material/walker.rs:21-27` `extract_vertex_colors(_scene, _shape, data, _inherited_props, mat)` — three dead params; the doc itself says "Pre-#438 this function ignored its `_mat` parameter and re-scanned…" i.e. the re-scan was removed and the parameters were left (26e11db0a, 2026-04-26 split). One caller: `import/mesh/ni_tri_shape.rs:158`.
  - `crates/nif/src/import/material/mod.rs:153` `is_decal_from_modern_shader_flags(flags1, _flags2, …)` and `:175` `is_two_sided_from_modern_shader_flags(_flags1, flags2, …)` (c8a0f313e, 2026-04-26, #712). Callers `dedicated_shader.rs:336,348,661,671` + tests.
  - `byroredux/src/material_translate.rs:1188-1189` `normal_alpha_spec_applies(material_kind, _metalness, _env_map_scale, …)` — body is `material_kind < 100 && normal_map_index != 0 && gloss_map_index == 0`; its two callers (:1245, :1296) and 6 test asserts still compute and pass `metalness`/`env_map_scale`. `:1266` `normal_alpha_spec_roughness(…, _glossiness, …)` (9 params, already under `allow(too_many_arguments)`; 10 call sites).
- **Not flagged (legitimate)**: `write_rgba_inplace(_ctx: GpuUploadCtx)` and `SkinComputePipeline::dispatch(_bound)` (capability-token params), `natives/{alias,forms,location}.rs` `_frame/_args/_statement` (fn-pointer table signatures), trait-impl `_args`/`_event_loop`.
- **Fix**: drop the params and update callers/tests; the `normal_alpha_spec_*` pair also sheds an `allow(clippy::too_many_arguments)` need.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD8-2026-10-25-03`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
