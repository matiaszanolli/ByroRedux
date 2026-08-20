# SF-2026-08-20-D9-01: the #3053 CDB gate makes the BGSM/BGEM resolver unreachable for any session with a Starfield CDB registered

**Issue**: #3230 · https://github.com/matiaszanolli/ByroRedux/issues/3230
**Finding ID**: SF-2026-08-20-D9-01
**Filed**: 2026-08-20 (comprehensive audit suite)

Filed from `docs/audits/AUDIT_STARFIELD_2026-08-20.md` § SF-2026-08-20-D9-01 (Dimension 9 — BGSM/BGEM external material flow).

**Severity**: LOW · **Status**: NEW — introduced by this delta's own fix (#3053)
**Location**: `byroredux/src/asset_provider/material.rs` — the `starfield_named_material` gate (`:1061-1063`) and the `return MergeOutcome::PresenceOnly` that closes its block (`:1115`), inside `merge_external_material`

## Description

#3053 widened the CDB-PBR gate from `.mat` to `.mat | .bgsm | .bgem` behind the same
provider capability check:

```rust
let starfield_named_material =
    path.ends_with(".mat") || path.ends_with(".bgsm") || path.ends_with(".bgem");
if starfield_named_material && provider.has_starfield_cdb() {
    material.is_pbr = true;
    ...
    return MergeOutcome::PresenceOnly;
}
```

That `return` sits **before** the BGSM/BGEM dispatch further down the function. So once
any Starfield CDB is registered on the provider, a `.bgsm`/`.bgem` path that *does*
resolve to a real file is never parsed: `from_bgsm` stays false, the BGSM
spec-glossiness translation is skipped, and every authored texture role, `glass_enabled`
flag and PBR scalar in that file is discarded in favour of the presence-only
`is_pbr = true` flip.

## Evidence

The early `return MergeOutcome::PresenceOnly` precedes every `resolve_bgsm` / BGEM call
site in the function — verified at `bb0b92f2`. The gate is a **provider** capability
(`provider.has_starfield_cdb()`), not a per-path one, so it is not narrowed by which
archive the mesh came from.

Today this is **vacuous for vanilla**: 0 `.bgsm` and 0 `.bgem` files across all 129
Starfield archives, which is exactly what motivated #3053 (Starfield's shipped NIFs use
`.bgsm`/`.bgem`-named references for materials that live in the CDB). So this is a
forward-looking narrowing, not a live regression — hence LOW.

## Impact

A Starfield mod that ships genuine BGSM/BGEM sidecars — or any mixed session where a
Starfield CDB is registered alongside FO4-era loose materials — silently loses all
authored material data for those meshes. The one-shot warn added by the same commit
makes it *worse* by describing the situation as
*"has no external BGSM/BGEM payload; using CDB-gated PBR fallback"* even when one
exists.

## Suggested Fix

Attempt `resolve_bgsm` / BGEM **first** for `.bgsm`/`.bgem` paths and fall through to
the CDB-gated PBR flip only on a resolve **miss**. That keeps #3053's whole benefit
(vanilla Starfield always misses) while restoring the resolver for the case where a
payload is actually present. The `.mat` arm is unaffected and should keep its current
early return.

## Related

- **#3053** — the fix that introduced this shape (CLOSED, verified in place at HEAD)
- #2601 — resolve-failure tracking at the merge site
- #2709 — why this arm returns `PresenceOnly` rather than `Merged`
- #2359 — CDB Phase 2; once the CDB is a per-field producer this arm should *overwrite* rather than short-circuit

## Completeness Checks
- [ ] **SIBLING**: the `.mat` arm's early return is deliberately kept; only the `.bgsm`/`.bgem` names change order
- [ ] **CANONICAL-BOUNDARY**: `merge_external_material` keeps its `&mut ImportedMaterial` signature — no widening to `ImportedMesh` (the `05d68926` narrowing)
- [ ] **TESTS**: a regression test registers a Starfield CDB *and* a resolvable `.bgsm`, and asserts the authored roles/scalars survive
