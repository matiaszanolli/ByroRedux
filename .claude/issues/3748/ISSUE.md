# #3748 — TD8-2026-08-30-03: `byroredux-mod-runtime` is a dangling `[workspace.dependencies]` alias with no member consumer

**Labels**: bug, low, tech-debt

---

- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `Cargo.toml:48` — `byroredux-mod-runtime = { path = "crates/mod-runtime" }`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD8-2026-08-30-03`), HEAD `64f64480`

## Description

The workspace root declares `byroredux-mod-runtime = { path = "crates/mod-runtime" }` in
`[workspace.dependencies]`, but **no member `Cargo.toml` contains
`byroredux-mod-runtime = { workspace = true }`** (re-verified at HEAD: the only other hit
is the crate's own `name =` line).

Every `[workspace.dependencies]` key was swept against every member manifest; this is the
only genuine orphan (`env_logger` and `lz4_flex` came back as regex artefacts and are
consumed by 10 and 1 members respectively).

## Scope note

The crate itself (1 475 LOC) is a **deliberate** consumer-less landing, documented in
`_audit-common.md`'s un-owned table and gated on the sandboxed-mod host milestone — **the
crate is not the finding**. The unused workspace-dependency *alias* is.

## Impact

A dangling alias makes `grep`-based consumer discovery report a dependency edge that does
not exist.

## Suggested Fix

Either wire the alias where the host will consume it, or drop the line until then.
Effort: trivial.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — re-run the full `[workspace.dependencies]`-vs-members sweep after the change
