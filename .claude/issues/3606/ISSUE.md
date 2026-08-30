# #3606 — REN-2026-08-30-D13-03: The Halton phase-count rationale is mathematically false and misidentifies the sample `% 8` missed — duplicated verbatim at two sites

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3606 --json state`.

---

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`taa_jitter` doc comment, lines 347–349; the identical inline copy in `draw_frame`, lines 2029–2033)
- **Status**: OPEN — doc-rot; the constant `16` itself is fine, the stated reason for it is not
- **Description**: Both copies read: *"Halton(2) natural period is 2, Halton(3) natural period is 3, LCM = 6. Using 16 (nearest power-of-2 ≥ 6) avoids the asymmetric Y-coverage gap that `% 8` caused (the 9th Halton(3) sample ≈ 0.889 was never reached with `% 8`)."* Both claims are wrong. (a) Halton sequences are aperiodic — the radical inverse `halton(index, base)` (`draw.rs:50–59`) is injective on `index`, so there is no "natural period 2/3" and no LCM to take. (b) `halton(9, 3) = 1/27 ≈ 0.037`, not `0.889`; `0.889 = 2/3 + 2/9 = halton(8, 3)`, and with `% 8` the index range is `(frame % 8) + 1 ∈ 1..=8`, so index 8 — and therefore `0.889` — *was* reached. The sample `% 8` actually omitted is index 9's `1/27`. As it happens `% 8` gave a perfectly stratified Y set (`{1,2,4,5,7,8}/9 ∪ {1,2}/3` = all eighths of the ninth-grid) while `% 16` adds four 27ths that are not aligned to that grid — so the comment's premise is not merely wrong, it is backwards about which is more uniform.
- **Evidence**:
  - `draw.rs:50–59` — textbook radical inverse; `halton(9,3)`: `9%3=0` (`+0`), `3%3=0` (`+0`), `1%3=1` (`+1/27`) → `0.037`. `halton(8,3)`: `8%3=2` (`+2/3`), `2%3=2` (`+2/9`) → `0.889`.
  - `draw.rs:375–376`: `let idx = (frame_counter % 16) + 1;` — the pre-`#1093` form was `% 8`, giving `idx ∈ 1..=8`, which includes 8.
  - The two comment blocks are byte-identical apart from `///` vs `//` prefixes, so any correction must be applied twice.
- **Impact**: No runtime effect — the sequence, the 1-indexing (`+ 1`, so the degenerate `halton(0, b) = 0` offset is correctly never produced), and the 16-entry wrap are all correct as written, and verified: indices 1..=16 in base 2 give all fifteen `k/16` plus `1/32`, and in base 3 give all thirds/ninths plus eight 27ths. The risk is purely that the false rationale invites a future "correction" toward the stated `LCM = 6`, or toward a different phase count on a premise that does not hold.
- **Suggested Fix**: Replace both copies with the real reason for 16 (a power-of-two wrap that keeps the base-2 dimension exactly stratified across the cycle, with a phase count comparable to the ~10-frame effective window implied by `alpha = 0.1` in `taa.rs::upload_params`), and drop the period/LCM sentence and the `0.889` claim. If the phase count is ever revisited on quality grounds, that is a measurement, not a comment edit — do not change `16` as part of this doc fix.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D13-03

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
