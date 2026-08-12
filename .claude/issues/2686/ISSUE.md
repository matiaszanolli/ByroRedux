# SAFE-D7-01: GLASS_RAY_BUDGET is a dead constant

**Issue**: #2686
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`

- **Severity**: MEDIUM
- **Dimension**: 7 — RT IOR-Refraction safety guards
- **Location**: [shader_constants_data.rs](crates/renderer/src/shader_constants_data.rs) (`GLASS_RAY_BUDGET`) · [ray_budget.rs](crates/renderer/src/vulkan/scene_buffer/ray_budget.rs) (`AdaptiveRayBudget::settings`) · [triangle.frag](crates/renderer/shaders/triangle.frag) (the `atomicAdd` gate)
- **Status**: NEW
- **Description**: The skill treats `GLASS_RAY_BUDGET` as the canonical
  runaway-recursion cap, mirrored into the generated
  `crates/renderer/shaders/include/shader_constants.glsl` and read by the
  shader. The Rust↔GLSL mirror *is* in lockstep (`2_097_152` ↔ `2097152u`), but
  **no shader references the define**: the live gate reads
  `rayBudget.glassRayLimit`, a per-frame CPU-uploaded word whose value comes
  from `AdaptiveRayBudget::settings()`'s four hard-coded tier literals
  (`262_144` / `524_288` / `1_048_576` / `2_097_152`), none of which reference
  `GLASS_RAY_BUDGET`. The canonical constant is therefore decorative: editing
  it changes nothing at runtime, and only the tier-3 literal happens to equal it
  today.
- **Evidence**:
  - `grep -rn GLASS_RAY_BUDGET crates/renderer/shaders/` → three hits, all
    comments or the `#define` itself; zero uses in shader code.
  - `triangle.frag`: `uint old = atomicAdd(rayBudget.rayBudgetCount, GLASS_RAY_COST);`
    then `glassIORAllowed = (old + GLASS_RAY_COST <= rayBudget.glassRayLimit);`
    — `GLASS_RAY_COST` *is* the generated define; `glassRayLimit` is not.
  - `ray_budget.rs`: four `glass_ray_limit:` literals, no import of
    `shader_constants_data`.
- **Impact**: The safety cap and its documented source of truth are decoupled.
  A future "lower the glass budget" change made in the obvious place
  (`shader_constants_data.rs`) silently does nothing, leaving the real cap at
  the tier table — the exact class of drift the generated-header mechanism was
  built to prevent. Also makes the shader comment at `triangle.frag`
  ("`GLASS_RAY_BUDGET` … from shader_constants.glsl") false for half its
  subject.
- **Related**: #1438 (the atomicAdd overshoot is by-design and is *not* reported
  here); `feedback_shader_struct_sync.md`.
  **Cross-audit**: overlaps **REN-D2-02** in the renderer audit.
- **Suggested Fix**: Derive the tier-3 `glass_ray_limit` from
  `shader_constants_data::GLASS_RAY_BUDGET` (and the lower tiers as fractions of
  it), or delete the const + generated define and document the tier table as the
  single source of truth. Either way add a test pinning the two together.

---

### LOW

---


---
*Filed from [`docs/audits/AUDIT_SAFETY_2026-08-12.md`](docs/audits/AUDIT_SAFETY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `SAFE-D7-01`.*

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
