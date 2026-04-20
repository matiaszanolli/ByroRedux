# Issue #480

FNV-REN-L2: Truncated comment fragment in triangle.frag:411 parallax section

---

## Severity: Low (docs)

**Location**: `crates/renderer/shaders/triangle.frag:411`

## Problem

Comment ends mid-sentence: `"mapping displaces \`fragUV\` before the base-albedo sample, and"` — no continuation. The code block afterward is correct, but the explanation is truncated.

Likely a copy/paste or merge hiccup in the parallax section.

## Impact

Docs only. Readers hit a dangling sentence.

## Fix

Finish the comment. Based on context: `"mapping displaces fragUV before the base-albedo sample, and the displaced UV is then used for all subsequent texture fetches (normal, metallic, roughness, AO)."`

## Completeness Checks

- [ ] **DOCS**: Sentence completed or rewritten
- [ ] **SIBLING**: Sweep triangle.frag for other truncated comments

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-REN-L2)
