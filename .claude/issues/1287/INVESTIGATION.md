# Investigation — #1287 audit-renderer says GpuMaterial 260 B; actual 300 B

## Premise — partly STALE (verified against current files)
The issue's primary claim — `.claude/commands/audit-renderer.md` Dim 14 says "260 bytes" —
was **already fixed** before this run: the Dim 14 checklist already read "now exactly
**300 bytes**" and even flagged the stale-named test + doc comments as drift candidates.
So fix #1 (the headline ask) was a no-op.

## What was still actionable
1. **Skill rationale was self-contradictory.** The Dim 14 text justified keeping the
   test name `gpu_material_size_is_260_bytes` "so a future size shift updates them in
   lockstep" — but lockstep already FAILED (name says 260, asserts 300). The *code's*
   actual rationale (`material.rs:41-42`, `audit-safety.md:66`) is "grep continuity"
   (the name is referenced by exact string across ~12 historical audit reports + issue
   snapshots). Rewrote the Dim 14 text to state the correct grep-continuity rationale.
2. **Three genuinely-stale size comments in `triangle.frag`** (the GLSL `GpuMaterial`
   mirror — the only shader that declares it): lines 100 & 108 said "260 B", line 174
   said "closes the 296 B struct". The companion code-doc cleanup (#1321 / TD7-NEW-01,
   CLOSED) fixed the Rust `material.rs` comments (now self-aware "asserted value is 300")
   but missed the shader. The skill explicitly flagged these as drift candidates.
   Fixed all three to 300 B; line 174 also wrongly implied `subsurface` closes the
   struct (it's `anisotropic` at offset 296→300). Updated the skill to drop the now-fixed
   "296 B doc comment" flag.

## Optional rename — DECLINED (with reason)
The issue's optional #2 (rename `gpu_material_size_is_260_bytes` → `gpu_material_size_pin`)
is counter-indicated: the name is referenced by exact string in ~6 live sites
(material.rs ×4, triangle.frag, audit-safety.md) and ~12 historical audit reports +
issue snapshots. The documented "grep continuity" rationale is the stronger call;
renaming would orphan the whole corpus. Kept the name; made the *rationale* honest instead.

## SIBLING scan (skill, other recent dimensions)
Spot-checked Dim 21 (Disney BSDF, the most recent / most-likely-to-drift): all pins
ACCURATE — `dielectricF0FromIor`@703 ✓, `deriveAxAy`@667 ✓, `MAT_FLAG_PBR_BSDF` #define
@204 ✓, gate-site count = 2 ✓. The skill mostly uses robust symbol-anchored references.
No drifted size/line pins found; exhaustive per-line re-verification of every dimension
is a dedicated `/audit-renderer` run, not this LOW fix.

## Scope / build impact
- Changed: `.claude/commands/audit-renderer.md` (skill markdown) + `triangle.frag`
  (comment-only). The `.spv` is pre-compiled and `include_bytes!`'d, so a comment-only
  GLSL edit does NOT change compiled output — no SPIR-V recompile, no `cargo` impact.
- Path-validate gate: PASS (687 refs across 24 skill files).

## Completeness checks
- **UNSAFE / DROP / LOCK_ORDER / FFI**: N/A.
- **SIBLING**: skill Dim 21 verified clean; the 3 stale shader comments (#1321's miss) fixed.
- **TESTS**: N/A — no logic change; the size pin already asserts 300 and passes. Test name
  deliberately retained (grep continuity), so the optional rename's test-run note doesn't apply.

## Related
#1321 / TD7-NEW-01 (Rust doc cleanup, CLOSED — missed the shader, now covered here),
#1200 / #1233 (sibling audit-skill spec-rot, both CLOSED).
