# #3571 — REN-2026-08-30-D10-02: the #3308 comparison gate can only be run *before* the conversion — `analyze_depth_field` is hardcoded to the conventional mapping in both its background test and its decode

**Labels**: `medium,renderer,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3571 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/core/src/ecs/components/camera.rs` (`analyze_depth_field` L317, `linear_distance_from_depth` L277, cleared test at L351); `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`)
- **Status**: New
- **Description**: `DEFAULT_RENDER_DISTANCE`'s doc block states the gate's
  contract as *"Run it before the conversion, run it after, and the far
  decades' `distinct_codes` are the before/after evidence — the thing that was
  otherwise unobservable and that made shipping reversed-Z speculative."*
  `depth_capture.rs`'s module doc repeats it (*"after a reversed-Z conversion —
  report the before/after difference"*), as does `commands/depth.rs`. The code
  cannot deliver the "after" half. Three separate sites are hardwired to the
  conventional near→0 / far→1 mapping, and there is no mapping selector on
  `analyze_depth_field`:
  1. the background classifier `if z >= 1.0 { stats.cleared += 1; continue; }`
     (L351) — under reversed-Z the clear value is `0.0`, so *nothing* would be
     classified as background and the frame's entire sky would decode into the
     bands, swamping exactly the far decade the gate reads;
  2. the decode `linear_distance_from_depth` (L277), whose
     `denom = 1.0 - z * (f - n) / f` inverts only the conventional
     `z_ndc(d) = f/(f-n)·(1 − n/d)` — there is no
     `linear_distance_from_depth_reversed` sibling to the
     `depth_resolution_at_reversed` that *was* added;
  3. `DepthBand::analytic_resolution` is always populated from
     `self.depth_resolution_at(mid)` and
     `analytic_resolution_reversed` always from the reversed sibling — after a
     conversion the two columns are swapped relative to reality, so the
     `depth.stats` table's "BU/step (reversed-Z would be)" header is then wrong
     in both columns.
- **Evidence**: read the full body of `analyze_depth_field`
  (`camera.rs:317-390`) — it takes only `&self` and `&[f32]`; `Camera` carries
  no reversed/conventional flag, and grepping `_reversed` in that file yields
  only `depth_resolution_at_reversed` and `DepthBand::analytic_resolution_reversed`
  (both analytic-only). `depth.rs`'s `execute` calls
  `camera.analyze_depth_field(&capture.samples)` with no mapping argument.
- **Impact**: The gate is half a gate. Its stated reason for existing is to
  make the reversed-Z conversion non-speculative by giving it a measurable
  before/after; whoever does that work will find the "after" run reports
  `cleared = 0`, a wildly inflated last-decade sample count, and nonsense
  `BU/step` columns, and will have to fix the analysis in the same change that
  they are trying to validate — precisely the position #3308 is trying to avoid
  being in. This is a design gap in brand-new code, not a live rendering bug.
- **Suggested Fix**: Add a mapping discriminant (a `DepthMapping::{Conventional,
  Reversed}` enum parameter on `analyze_depth_field`, or a `reversed: bool`
  field on `Camera` set by whatever sets the projection) and route all three
  sites through it: cleared test becomes `z >= 1.0` / `z <= 0.0`, decode picks
  between `linear_distance_from_depth` and a new reversed inverse
  `d = n / (z·(1 − n/f) + n/f)`, and the two `analytic_*` columns are labelled
  "current mapping" / "other mapping" rather than fixed. Adding the reversed
  inverse also lets `depth_decode_round_trips_the_projection` cover the
  reversed encode, which today it cannot.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D10-02

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
