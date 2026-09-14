# #4388 — TD8-009: `MergeOutcome`'s test-only `allow(dead_code)` justification is obsolete

**Labels**: low, nifal, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4388

- **Severity**: LOW · **Dimension**: 8
- **Location**: `byroredux/src/asset_provider/material/merge.rs:45-66` · **Status**: NEW (follow-on to closed #4289 / #2709) · **Effort**: trivial · **Kind**: tech-debt
- **Finding**: `resolved()` / `merged()` are kept "so the deferred telemetry sink #2709 asks for has something to call". #4289's `trace_merge_outcome` (`:88`) landed comparing `== MergeOutcome::PresenceOnly` directly, so every use of the two methods is in `byroredux/src/asset_provider/tests/`.
- **Suggested Fix**: Put `#[cfg(test)]` on the impl block and drop the comment.

Minor, unfiled notes from the Dim 8 allow-site table:
- `crates/bsa/src/ba2.rs:165` gates `end_mip` on "M40 streaming", but M40 (cell streaming) is closed and mip streaming is M39.
- `ActionBindings::bind_key` and the `VF_*` schema bits would read more honestly as `#[cfg(test)]` / `cfg_attr(not(test), …)`.
- `crates/plugin/examples/sf_smoke.rs:112`'s comment is slightly off.
- All other `allow(dead_code)` sites are justified: RAII guards, debug-gated items, std430 header fields, and hkx's `global_target`.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **CANONICAL-BOUNDARY**: Material-merge semantics stay at the NIFAL parser→`Material` boundary; nothing pushed into shaders/renderer. See `/audit-nifal`.
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
