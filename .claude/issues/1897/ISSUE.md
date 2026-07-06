# NIF-D2-05: ShaderFlags typed view + has_shader_property_fo3_fields are transitively dead in production

**Issue**: #1897 · **Severity**: LOW · **Labels**: low, nif-parser, nif, bug
**Dimension**: Version Gating · **Filed from**: docs/audits/AUDIT_NIF_2026-07-06.md (nif-deep suite)
**Location**: crates/nif/src/shader_flags.rs (ShaderFlags + classify/is_decal/is_two_sided),
version.rs (has_shader_property_fo3_fields), stream.rs (variant() accessor)

## Description
version.rs keeps has_shader_property_fo3_fields citing a "live consumer (shader_flags.rs)", but
ShaderFlags has ZERO production callers — every classify/is_decal/is_two_sided call is in #[cfg(test)].
variant() has no production call site either. Production material importer reads raw flags directly.
All three pub → no dead-code warning → survived the #1840 sweep (counted the test caller as live).

## Evidence
ShaderFlags::classify → only shader_flags.rs tests (648+); .variant() → no production site;
has_shader_property_fo3_fields → only shader_flags.rs:575 (inside classify) + version.rs:551 comment.

## Suggested Fix
Wire classify into import/material decal/two-sided path (genuine consumer), OR delete ShaderFlags +
has_shader_property_fo3_fields + variant(). Either way fix the wrong "live consumer" comment.
Follow-through on #1840, not a regression.
