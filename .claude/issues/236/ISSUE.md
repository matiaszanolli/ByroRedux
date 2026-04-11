# Issue #236 — NIF-04-11-L5

**Title**: has_shader_emissive_color() comment misleadingly implies FO76/Starfield are missing a feature
**Severity**: LOW (documentation)
**Dimension**: Version Handling
**Audit**: `docs/audits/AUDIT_NIF_2026-04-11.md`
**Labels**: low, nif-parser, documentation

## Location
`crates/nif/src/version.rs:148-155`

## Summary
The predicate exclusion of FO76/Starfield is semantically correct (those games never query this flag because they use `BSLightingShaderProperty`, not `BSShaderPPLightingProperty`), but the doc comment says "Present in FNV+ (user_version_2 >= 34)" which invites the same misreading that produced the real bug in #169 (`compact_material` / `has_emissive_mult`).

## Fix
Append a clarifying paragraph to the doc comment cross-referencing #169, OR rename the predicate to `has_pp_shader_emissive_color` so the PP scope is explicit.

## Related
#169 — real bug with the same visual pattern on sibling predicates.

## Completeness checks
- [ ] SIBLING: audit all `version.rs` feature flags for similar misleading comments

## Fix with
`/fix-issue 236`
