# TD2-115: Bitangent-sign clamp idiom duplicated across 4 sites, 2 files

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `crates/nif/src/types.rs:161-165` (`bitangent_sign`) vs.
`crates/nif/src/import/mesh/bs_geometry.rs:188` (new in #2246); plus 2 more
inline repeats in `bs_geometry_tangent_tests.rs`.
**Labels**: low, nif-parser, tech-debt, bug
**Source**: `docs/audits/AUDIT_TECH-DEBT_2026-08-03.md`

## Description
`bitangent_sign`'s tail idiom (`if dot < 0.0 { -1.0 } else { 1.0 }`) is
reproduced verbatim by #2246's fix (`if xyzw[3] < 0.0 { -1.0 } else { 1.0 }`)
— same invariant (clamp a signed value to exactly ±1), two different inputs,
no shared symbol. The commit message for #2246 explicitly frames this as
matching `bitangent_sign`'s output convention without sharing a function. The
test file repeats the same ternary two more times as an inline "simulation"
per its own comments, rather than calling production code.

## Impact
No correctness bug today (all sites agree). Risk is future convention drift
(e.g. a change to zero-tie-break behavior, or a migration to `f32::signum`
which disagrees on `-0.0`) requiring a grep across 4 sites instead of 1 — the
same divergent-fix-history failure mode the project's already-closed TD2-001
addressed in this same subsystem.

## Related
TD2-001 (closed) — same failure class, different call sites.

## Suggested Fix
Add a small shared `clamp_sign(x: f32) -> f32` helper beside `bitangent_sign`
in `crates/nif/src/types.rs`; call it from both production sites. Low
urgency — reasonable to batch with the next tangent-path touch.

## Age / Effort
`bs_geometry.rs` site is 1 day old (today's #2246); `bitangent_sign` predates
this window. Effort: small.
