# #1344 — D5-02: FNV Prospector synthesized-collider growth + super-linear fence cost

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d5-02). GitHub is authoritative for live state — query `gh issue view 1344 --json state`._

**Severity**: MEDIUM · **Dimension**: Real-Data Validation / Performance · **Source**: AUDIT_FNV_2026-05-30 (D5-02)

**Location**: `ROADMAP.md:22,30-40` (follow-up note) ; suspected synthesized-static-collider fallback gate (`#1294`-adjacent, M28.5)

**Description**: ROADMAP's 2026-05-28 refresh notes the FNV Prospector Saloon interior grew +37% entities (2564 → 3507) with `fence` ballooning 2.62 → 11.65 ms (super-linear vs the entity growth), attributed to the FNV/FO4 synthesized-static-trimesh-collider fallback (each adds an RT BLAS). ROADMAP explicitly states this "needs a fix-issue to confirm the collider count is intended and bound its RT cost." No tracking issue exists — this is that issue.

**Evidence**: ROADMAP.md:22 `Prospector Saloon ... 71.4 FPS / fence=11.65 / 3507 ent` vs prior `161.4 / fence=2.62 / 2564 ent`. Live run this audit: `fence=11.16 ms` (82% of `wall_ms=13.62`) on a 1224-draw interior, log `M28.5 static collider AABB: ... (469 fixed colliders); rapier_bodies=580`. Exterior is healthy by contrast (fence 3.10 ms). NOTE: a follow-up exterior bench at radius 12 showed the high-entity regime is CPU-bound on `systems_ms` (transform propagation), distinct from this interior fence cost.

**Impact**: −56% FPS vs the prior Prospector record on the reference title's canonical interior. Still playable (71 FPS) but the cost is super-linear and will worsen on denser interiors.

**Suggested Fix**: Instrument the synthesized-collider count per FNV interior; confirm the gate (#1294-adjacent) intends one collider per architecture STAT; then either (a) coalesce architecture trimeshes into fewer BLAS, or (b) exclude static architecture from the RT BLAS set where it doesn't contribute to traced lighting. Performance-correctness item, not a render bug.

## Completeness Checks
- [ ] **SIBLING**: Confirm FO4 MedTekResearch01 (+42% entities, ROADMAP:24) shares the same collider-fallback cause.
- [ ] **TESTS**: Add a telemetry assertion / runtime-audit baseline pinning the synthesized-collider count for Prospector so a future gate change is caught.
