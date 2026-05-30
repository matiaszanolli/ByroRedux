# #1348 — D4-01: Streaming RIS reservoir count is 16, docs say 8

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d4-01). GitHub is authoritative for live state — query `gh issue view 1348 --json state`._

**Severity**: LOW · **Dimension**: RT Lighting Pipeline (doc drift) · **Source**: AUDIT_FNV_2026-05-30 (D4-01)

**Location**: `crates/renderer/shaders/triangle.frag:2664` (code) vs `CLAUDE.md` / `ROADMAP.md` (docs)

**Description**: CLAUDE.md, ROADMAP, and the audit briefs all state "streaming RIS M31.5 (8 reservoirs/fragment)". The live shader uses `const uint NUM_RESERVOIRS = 16` (Phase 19 upgrade, documented inline at triangle.frag:2655-2664). The unbiased W estimator (`W = resWSum/(K·w_sel)`, invK at :2983) and the 64× clamp are intact; 16 is strictly more accurate. Pure doc drift, not a code regression.

**Evidence**: triangle.frag:2664 `const uint NUM_RESERVOIRS = 16;`; inline comment :2655 "NUM_RESERVOIRS 8 → 16. Doubles the WRS shadow-ray count per fragment".

**Impact**: Cosmetic. The stale doc could mislead a future audit into "fixing" 16 back to 8.

**Suggested Fix**: Update CLAUDE.md / ROADMAP "8 reservoirs/fragment" → "16 reservoirs/fragment (Phase 19)". No shader change.

## Completeness Checks
- [ ] **SIBLING**: Grep all docs (CLAUDE.md, ROADMAP.md, docs/engine/renderer.md, audit skills) for "8 reservoirs" and update in lockstep.
