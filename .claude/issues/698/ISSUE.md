# Issue #698: O5-5: meshes/marker_radius.nif is the sole hard-fail; should be promoted to truncation for sweep-headline consistency

**Severity**: MEDIUM (cosmetic but affects audit headline)
**File**: `meshes\\marker_radius.nif` (vanilla Oblivion debug marker)
**Dimension**: Real-Data Validation

`marker_radius.nif` (1,822 bytes) returns `Err(...)` rather than truncated — the only file in the 2026-04-25 sweep that does so. Inspecting the failing block (a leading NiNode requesting 318,767,103 bytes) shows it is corrupt-by-design (debug marker).

**Options**:
- (a) Recognize the debug-marker class and skip it in the sweep, OR
- (b) Convert the alloc-cap rejection from `Err(...)` to `truncated: true` so the gate semantics stay consistent across the corpus.

Net impact: zero on game content (no cell references it), but it is the difference between "0 failures" and "1 failure" headline numbers and breaks the "truncation = recoverable, error = parser bug" mental model.

**Fix**: prefer (b) — `check_alloc` failure should mark `truncated: true` and continue rather than bubbling up as an unrecoverable error. The corpus then has "0 hard failures, 384 truncated."

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
