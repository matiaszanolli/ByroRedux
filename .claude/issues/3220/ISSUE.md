# #3220 — SKY-2026-08-20-D7-01: the Skyrim underwater-fog clamps erase 22 authored negative near planes and turn HelgenWater's authored (-1000, -172) into a degenerate 1-unit fog span

**Issue**: #3220 — https://github.com/matiaszanolli/ByroRedux/issues/3220
**Finding ID**: `SKY-2026-08-20-D7-01`
**Severity**: MEDIUM
**Dimension**: 7 — WATAL canonical translation (Skyrim slice)
**Audit**: `/audit-skyrim` — `docs/audits/AUDIT_SKYRIM_2026-08-20.md` (HEAD `bb0b92f2`, 2026-08-20 comprehensive suite)
**Labels**: medium, legacy-compat, import-pipeline, bug
**Filed**: 2026-08-20 · `/audit-publish`

---

**Audit**: `/audit-skyrim` — `docs/audits/AUDIT_SKYRIM_2026-08-20.md` (Dim 7 — WATAL canonical translation, Skyrim slice), HEAD `bb0b92f2`
**Finding ID**: `SKY-2026-08-20-D7-01`

- **Severity**: MEDIUM
- **Status**: NEW
- **Changed File**: yes — `crates/plugin/src/esm/records/misc/water.rs` is #2 on this delta's hot-file list

## Location

- `crates/plugin/src/esm/records/misc/water.rs:770-771` (`apply_skyrim_dnam_tail`)
- Consumer: `byroredux/src/systems/water.rs:469-476` + `underwater_extinction`

## Description

The decoder reads the under-water fog pair and then clamps:

```rust
p.underwater_fog_near = near.max(0.0);
p.underwater_fog_far  = far.max(p.underwater_fog_near + 1.0);
```

Skyrim authors **negative** under-water near planes as a matter of course, and the second clamp is not a sentinel — **it fabricates a value**.

The canonical layer already has a correct answer for an inconsistent pair: leave the `0.0` sentinel, which `systems/water.rs:469` detects and falls back to `mat.fog_near` / `mat.fog_far`:

```rust
let (fog_near, fog_far) = if mat.underwater_fog_far > mat.underwater_fog_near {
    (mat.underwater_fog_near, mat.underwater_fog_far)
} else {
    (mat.fog_near, mat.fog_far)
};
```

The clamp defeats that fallback by manufacturing a pair that passes the test.

## Evidence

All 34 `Skyrim.esm` `WATR` records, `DNAM[144]` / `DNAM[148]` (layout confirmed by the distribution — 11 distinct near values, 10 distinct far, monotone per record):

```
near < 0 on 22/34 records, spanning 8 distinct authored values (-10000 … -40)
all 22 collapse to near = 0.00

DefaultMarshWater      near = -10000  far = 1000   -> 0.00 / 1000   (span 11000 -> 1000)
RiftenWater            near = -10000  far = 1600   -> 0.00 / 1600
DefaultMarshWaterTrans near =  -4000  far = 1200   -> 0.00 / 1200
DefaultWater           near =   -500  far = 1600   -> 0.00 / 1600
HelgenWater            near =  -1000  far =  -172  -> 0.00 /    1.00   <-- degenerate
```

`HelgenWater` (`000C1D45`) is the only record with **both** planes negative. Its parsed output is `(0.0, 1.0)`, which passes the `far > near` fallback test, yields `span = 1.0` in `underwater_extinction`, and therefore saturates — `ramp` clamps to 1 and extinction reaches `1 - e^-2 ~ 0.86` — at **one unit of depth**.

The shader's own ramp gate agrees it is live:
`water.frag:458  bool hasUnderwaterRamp = push.scroll_c.w > push.scroll_c.z + 0.001;`

## Impact

**Helgen's water renders as an opaque wall the instant the camera submerges — the game's opening area.**

The other 22 negative-near records lose their authored ramp offset (fog that should already be partly applied at the eye starts clear at the surface instead), flattening `DefaultMarshWater`'s authored 11 000-unit span to 1 000.

Skyrim-scoped in effect: this is the only decoder arm that clamps this pair against negative authored data. (The same `.max(0.0)` / `.max(near + 1.0)` shape is repeated at `water.rs:400-403`, `:655-658`, `:948-951`, `:1093-1096` and `:1197-1200` for the other games — worth checking whether any of those corpora also author negatives, but this audit measured Skyrim only.)

## Related

- **#3104** — the `DNAM` one-field misalignment in the *same function*; fix them in one pass over `apply_skyrim_dnam_tail`.
- **#3148** — the only real-data `WATR` guard runs *after* every clamp, so it cannot detect this class at all.
- #2790 / #2785 — `WaterMaterial::fog_near` travel.

## Suggested Fix

**Reject rather than repair.** When `far <= near`, leave both fields at the `0.0` sentinel so the documented `fog_near` / `fog_far` fallback engages. Keep the authored **sign** on `near`: the ramp arithmetic in `underwater_extinction` is `(depth - near) / span`, which is well-defined for negative `near` and is exactly the authored intent.

## Completeness Checks
- [ ] **SIBLING**: the same clamp pair appears at `water.rs:400-403`, `:655-658`, `:948-951`, `:1093-1096`, `:1197-1200` — decide per game from that game's corpus, do not blanket-apply
- [ ] **CANONICAL-BOUNDARY**: the sentinel-vs-authored decision stays at the parser boundary; `systems/water.rs` must keep its fallback rather than gaining a second repair site
- [ ] **TESTS**: a fixture with `near = -1000, far = -172` asserts the sentinel survives (and a second with `near = -10000, far = 1000` asserts the negative near is preserved, not zeroed)
- [ ] **VERIFY-WITH**: #3104 touches the same function — land and re-measure together
