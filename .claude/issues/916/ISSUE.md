---
issue: 0
title: REN-D9-NEW-03: Glass IOR ray-budget under-counts the multi-passthru loop (2 claimed, 4 worst-case)
labels: renderer, medium, vulkan, performance
---

**Severity**: MEDIUM (RT budget telemetry off by 2× under documented worst case)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 9)

## Location

- `crates/renderer/shaders/triangle.frag:1470-1475` — budget claim site (`atomicAdd 2u`)
- `crates/renderer/shaders/triangle.frag:1597-1637` — passthru loop (up to 3 iterations per refraction)

## Why it's a bug

The Phase-3 IOR glass branch claims 2 ray slots per fragment from the per-frame budget:

```glsl
const uint GLASS_RAY_BUDGET = 8192u;
if (glassIORAllowed) {
    uint old = atomicAdd(rayBudget.rayBudgetCount, 2u);
    glassIORAllowed = (old + 2u <= GLASS_RAY_BUDGET);
}
```

But the b38d16b glass-passthrough loop fires up to 3 refraction-ray iterations per fragment (`REFRACT_PASSTHRU_BUDGET = 2`). On stacked glass-on-glass scenes (the exact regression #789 was filed for) all 3 iterations fire — the worst-case ray cost is **4 rays per fragment** (1 reflection + 3 refraction), but the budget tracker only debits **2**.

Worst-case: ~2× the documented ceiling at the documented density target. Hardware impact is bounded (atomic still terminates the per-frame ray flood), but for any future RT-budget tuning UI / telemetry counter the bookkeeping reports the wrong number.

## Fix sketch (cheapest to most thorough)

1. **Comment-only**: document the worst-case is 2-4 rays per fragment; keep budget at 8192. Simplest.
2. **Tighten the claim**: `atomicAdd(rayBudget.rayBudgetCount, 4u)` instead of 2. Doubles the cliff density but accounts honestly. Visible IOR glass band moves from ~10% → ~5% of fragments under the doc's load model.
3. **Fine-grained accounting**: track passthru iteration count, claim additional slots inside the loop. Most accurate but adds per-iteration `atomicAdd` cost (3-way atomic contention on glass-heavy frames is non-trivial — single atomic per fragment was the #789 follow-up's whole point).

Option 1 or 2 has the right cost/benefit. Option 3 only matters once a budget telemetry overlay exists.

## Completeness Checks

- [ ] **SIBLING**: Verify reflection ray budget is correctly accounted (single ray per fragment).
- [ ] **TESTS**: Visual repro: stacked-glass scene, verify the documented density cliff matches new accounting.

## Related

- #789 (glass IOR same-texture passthru identity skip) — the loop this finding measures.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
