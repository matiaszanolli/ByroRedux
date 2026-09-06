# #3807 — EX-14/15 item A: ground-cover density streaming (GRAS/REGN scatter, chunking, LOD, RT proxy)

**State**: OPEN · **Labels**: enhancement ecs renderer legacy-compat terrain-exterior 

Split from issue 2369 (EX-14/15 item A) per that issue's own plan doc,
[`docs/engine/exterior-readiness-plan.md`](../../docs/engine/exterior-readiness-plan.md#ex-1415-ground-covertrees-persistent-refs-fo4-spatial-data),
which explicitly recommends separate tracking sub-issues per sub-thread
rather than one PR for the whole epic.

## Status (verified 2026-08-31): NOT DONE

Phase 0 (canonical types, LTEX keyword map, palette) already landed
(2026-08-12) — `crates/core/src/ecs/components/groundcover.rs` /
`byroredux/src/groundcover_translate.rs`. Everything past that is
unstarted. `GRAS` still parses through `parse_minimal_esm_record`
(`crates/plugin/src/esm/records/dispatch_misc_stub.rs`), every field
discarded — no real consumer exists yet.

## Scope (per [`exal-groundcover.md`](../../docs/engine/exal-groundcover.md) §9)

1. **§11.1 blocking measurement (do this FIRST, no code before it)** —
   bench the terrain-attribute-sampling indirection cost (global-vertex-
   SSBO read per candidate point vs. a baked per-cell attribute texture)
   via `--bench-hold` against real terrain. Required by the design doc's
   own gate and this project's no-guessing policy — measure, don't
   estimate.
2. **Phase 1 — scatter**: `ExcludedFromTlas` generalisation, chunking
   (size TBD by the §11.2 sweep), the density field in GLSL,
   `groundcover_scatter.comp`, debug point rendering over real terrain.
3. **Phase 2 — blades + wind** (near tier only).
4. **Phase 3 — LOD chain**, tier-3 (always-on detail layer) lands first
   so later tiers are authored against a correct backdrop.
5. **Phase 4 — RT proxy shell.**
6. **Phase 5 — per-game palette**: real `GRAS` field decode (replacing the
   `MinimalEsmRecord` stub) → species resolution. This is where GRAS
   finally gets a real consumer.
7. **`OwnershipTracker` classes** for ground-cover blade/chunk buffer byte
   counts, added alongside Phase 1 — none exist today, so the EX-08 soak
   currently can't distinguish a ground-cover leak from anything else.

## Related
Issue 2369 (parent, split). Design doc: `exal-groundcover.md`. Coordinates
with issue 3301 (REGN's `Grass`/`Objects` RDAT kinds overlap this scope —
coordinate, don't duplicate).
