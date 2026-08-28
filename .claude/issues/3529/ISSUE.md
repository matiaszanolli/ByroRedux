# #3529 — SPT-2026-08-28-D2-01: the [16, 8192] billboard-size clamp is NaN-transparent, so a non-finite BNAM produces a NaN quad, NaN LocalBound and NaN BLAS vertices

**Labels**: medium, speedtree, terrain-exterior, bug
**Filed from**: `docs/audits/AUDIT_SPEEDTREE_2026-08-28.md` (`/audit-publish`, 2026-08-28)

---

**Severity**: MEDIUM
**Dimension**: Placeholder Fallback
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-28.md` — SPT-2026-08-28-D2-01

**Location**: `crates/spt/src/import/mod.rs:232-249` (`compute_billboard_size`), specifically
`:238-242`; consumed at `:167` → `placeholder_billboard_mesh` (`:279-345`)

## Description

`compute_billboard_size` documents its clamp as the corrupt-input guard — *"All paths clamp to
the `[16, 8192]` band so corrupt input can't produce a 1-pixel mosquito or a floor-to-skybox
planet-sized billboard"* (`:228-230`). The BNAM tier is:

```rust
// crates/spt/src/import/mod.rs:238-242
if let Some((w, h)) = params.billboard_size {
    let width = w.abs().clamp(16.0, 8192.0);
    let height = h.abs().clamp(16.0, 8192.0);
    return (width, height);
}
```

`f32::clamp` is NaN-transparent: it returns `self` unchanged when `self` is NaN. Verified
empirically (`rustc -O`, `f32::NAN.abs().clamp(16.0, 8192.0)` → `NaN`, `is_nan() == true`).

The two sibling tiers are both immune by construction — `params.bounds` reaches the cell route
only as `i16`→`f32` from `ObjectBounds` (`references/import.rs:322-326`, never NaN), and the
MODB tier is explicitly filtered (`.filter(|r| *r > 0.0)`, `:243`, which NaN fails). **BNAM is
the one tier fed by a raw, unvalidated `f32` read off disk** — `parse_tree`'s
`find_sub(subs, b"BNAM") … Some((r.f32().ok()?, r.f32().ok()?))`
(`crates/plugin/src/esm/records/tree.rs:175-181`) does no finiteness check.

A NaN width/height propagates straight into `placeholder_billboard_mesh`'s `positions`, into
`local_bound_center`/`local_bound_radius` (`:339-340`), and from there into the ECS `LocalBound`
insert (`byroredux/src/cell_loader/spawn/mesh_instance.rs:761-769`, which has no finiteness
gate) and the batched GPU vertex upload. The `is_finite` guards that do exist in `spawn.rs`
(`:112-126`, `:191-199`, `:240`) all belong to the packed-Havok proxy synthesiser and are never
on this path.

## Evidence

- `crates/spt/src/import/mod.rs:238-242` (quoted above) vs. `:228-230`'s stated contract.
- `crates/spt/src/import/mod.rs:243` — the MODB tier's `> 0.0` filter, which *does* reject NaN,
  demonstrating the pattern was applied inconsistently rather than deliberately omitted.
- `crates/plugin/src/esm/records/tree.rs:175-181` — BNAM read with no finiteness check.
- `crates/spt/src/import/mod.rs:633-645` — the existing guard `bnam_clamps_to_safe_band` covers
  negative (`-500.0`) and oversized (`50_000.0`) BNAM but not NaN, so the hole is untested.
- Live `rustc` check of `f32::NAN.abs().clamp(16.0, 8192.0)` → `NaN`.

## Impact

A malformed BNAM yields four NaN vertex positions and a NaN bounding sphere. Downstream that is
a NaN `WorldBound` (which `bounds.rs`'s parent-fold then propagates up the placement hierarchy),
NaN frustum-cull comparisons, and NaN vertices in a static BLAS build — which is undefined
behaviour on the Vulkan side, not merely a visual artifact.

**Reachability is mod-content-only**: BNAM is consumed only when OBND is absent, and vanilla
FO3/FNV ship OBND on 100 % of TREE records while Oblivion ships no BNAM at all. Rated MEDIUM on
that basis (missing error handling on a recoverable path) rather than higher; escalate if the
NaN is ever shown to reach an acceleration-structure build in practice.

## Related

- #3194 — the *identical* NaN-transparency class in the other SpeedTree consumer
  (`apply_speedtree_wind`'s gust), which was filed and fixed with exactly the guard missing here
  (`billboard.rs:218`, `let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };`).
- #1002 — the audit that added the BNAM tier and its clamp.

## Suggested Fix

Filter the BNAM tier the way the MODB tier already is —
`params.billboard_size.filter(|(w, h)| w.is_finite() && h.is_finite())` — so a non-finite pair
falls through to the next tier instead of poisoning the quad. Extend `bnam_clamps_to_safe_band`
(or add a sibling) with a `(f32::NAN, f32::NAN)` case asserting the default 256 × 512 fallback.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the other `clamp`-as-guard sites in `crates/spt/src/import/mod.rs`, and the ungated `LocalBound` insert in `spawn/mesh_instance.rs`
- [ ] **TESTS**: A regression test pins this specific fix (a `(f32::NAN, f32::NAN)` BNAM case asserting the 256 × 512 default)
