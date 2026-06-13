## Finding REN2-16 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: Tangent-Space & Normal Maps (doc-rot)
- **Location**: `crates/renderer/src/shader_constants.rs:199-207` (doc-comment; test array at `:211-225`; companion test at `:46`)
- **Status**: NEW (adjacent to, but distinct from, open #1482). Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

The doc-comment for `triangle_frag_dbg_bits_not_redeclared` says the shader "must NOT redeclare any of the 10 DBG_* bit flags" — the test array actually iterates **13** flags. It also claims "Positive coverage that the value flows through correctly lives in `generated_header_contains_all_defines` (verifies each #define is emitted with the right value)" — that companion only mirrors **4** of the 13 DBG flags (DBG_BYPASS_POM, DBG_VIZ_NORMALS, DBG_BYPASS_NORMAL_MAP, DBG_DISABLE_HALF_LAMBERT_FILL); the gap is exactly open issue #1482.

## Suggested Fix

Correct the count (or make it count-free: "the DBG_* flags listed below") and soften the companion-coverage claim to reference #1482's actual 4-of-13 state — or close the gap together with #1482.

## Completeness Checks
- [ ] **SIBLING**: Check other lockstep-test doc-comments for hardcoded counts
- [ ] **TESTS**: N/A (doc-only) unless folded into the #1482 pin expansion

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
