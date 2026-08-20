# NIF-D4-2026-08-20-03: the #2632/#2828 degenerate-tangent guard emits a zero tangent for exactly the case its own comment names as the motivation

Issue: https://github.com/matiaszanolli/ByroRedux/issues/3176
Finding: NIF-D4-2026-08-20-03
Labels: low,nif-parser,nif,bug
Source: docs/audits/AUDIT_NIF_2026-08-20.md

Filed from `docs/audits/AUDIT_NIF_2026-08-20.md` (Dimension 4 — Geometry Extraction & Import Handoff).

**Severity**: LOW — the shader has a documented, legitimate fallback for a zero tangent, so the observable effect is "synthesis silently declines", not corruption and not NaN.
**Game Affected**: all — both producers. Z-up (`synthesize_tangents`) serves Oblivion / FO3 / FNV `NiTriShape` and Skyrim SE+ `BSTriShape` without `VF_TANGENTS`; Y-up (`synthesize_tangents_yup`) serves Starfield `BSGeometry` and SSE-reconstructed geometry.

**Location**: `crates/nif/src/import/mesh/tangent.rs:275-292` (Z-up, added by `8075133c` / #2828) and `:481-502` (Y-up, added by #2632).

## Description

Both degenerate branches take a cyclic permutation of the normal as a seed tangent, Gram-Schmidt it against `N`, and normalize:

```rust
let t_y_raw = [n_yup[1], n_yup[2], n_yup[0]];               // :489
let dot_nt  = n_yup[0]*t_y_raw[0] + n_yup[1]*t_y_raw[1] + n_yup[2]*t_y_raw[2];
let mut t_y = [ t_y_raw[0] - n_yup[0]*dot_nt, … ];
normalize_inplace(&mut t_y);
let b_y = cross(n_yup, t_y);
```

The #2632 comment directly above states the reason for the projection: *"a raw cyclic permutation of N's components is NOT generally orthogonal to N (e.g. **any N with all-equal components permutes to itself**)"*.

But for precisely that input the projection removes the entire vector: `t_y_raw == N` => `dot_nt == 1` => `t_y == [0,0,0]`, and `normalize_inplace` maps a below-`1e-12` vector to `[0.0, 0.0, 0.0]` (`tangent.rs:550-560`) rather than picking a different seed. `b_y = cross(N, 0) = 0` follows, and `bitangent_sign(N, 0, 0)` returns `+1.0` via `clamp_sign(0.0)` (`crates/nif/src/types.rs:154-162`). The vertex ships `[0.0, 0.0, 0.0, 1.0]`.

The Z-up sibling has the identical algebra: the coordinate swap is orthogonal, so `dot(n_yup, t_y_raw) == dot(n_zup, t_z)` and the trigger condition is unchanged.

## Evidence

`N = ±(1,1,1)/sqrt(3)` is the trigger, and it is representable in every source encoding the branch sees. `BSTriShape` normals are three independent `byte_to_normal` reads (`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1194-1197`), so any vertex whose three normal bytes are equal — the ordinary encoding for a diagonal/corner-chamfer normal — lands exactly on it.

The branch is additionally gated on `vec3_is_zero(&tangent_zup) || vec3_is_zero(&bitangent_zup)` i.e. degenerate UVs, so the two conditions must coincide; that makes it rare, not unreachable.

Pre-#2828 / pre-#2632 the branch returned the raw permutation, which is non-orthogonal but never zero — so the guard traded a slightly-wrong basis for **no basis at all** in this one case.

## Impact

Bounded. `triangle.frag`'s documented contract (`crates/renderer/shaders/triangle.frag:446-451`) is that a zero `fragTangent.xyz` falls back to the screen-space derivative TBN (Path 2), so the affected vertices get a valid — just not authored-quality — basis. No NaN, no corruption. The cost is that `synthesize_tangents`' whole purpose is defeated on those vertices, silently.

## Suggested Fix

When `t_y` collapses below the `normalize_inplace` threshold, fall back to a second, non-parallel seed — the standard choice is the world axis least aligned with `N` (pick the smallest of `|N.x|` / `|N.y|` / `|N.z|`, cross with `N`), which is orthogonal by construction and cannot degenerate. Add the `N = (1,1,1)/sqrt(3)` case to `crates/nif/src/import/mesh/tangent_convention_tests.rs`, whose existing #2632 guard tests (`:229`, `:290`) use normals that do not trigger it.

## Related

- #2828 (CLOSED, Z-up half), #2632 (CLOSED, Y-up half). This is **not** a regression of either — the code is exactly as they landed it — it is an incompleteness in the fix they both implement.
- Distinct from OPEN #2815 (`perturbNormal` Path 1 NaN when tangent is *parallel* to normal) — that is a renderer-side guard for a non-zero degenerate tangent; this is a producer-side zero.
- Sibling of the Z-up normalize asymmetry filed alongside this one (same function pair).

## Completeness Checks
- [ ] **SIBLING**: the fallback seed is applied to **both** producers (`:275-292` Z-up and `:481-502` Y-up) — they have identical algebra
- [ ] **TESTS**: `tangent_convention_tests.rs` gains an `N = (1,1,1)/sqrt(3)` + degenerate-UV case asserting a non-zero, unit, N-orthogonal tangent
- [ ] **NO-REGRESSION**: existing #2632 guard tests at `:229` / `:290` still pass unchanged
