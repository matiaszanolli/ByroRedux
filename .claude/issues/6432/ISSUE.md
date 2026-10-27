# CHAR-2026-10-26-L-12: The FO4 capture says both "the actual value is the float (93.5)" and `TotalHP = floor(…)`; the code takes the floor

- **Severity**: LOW
- **Dimension**: Derived Formulas (RoundMode)
- **Game**: fo4
- **Location**: `docs/engine/charal-fo4-ruleset.md:76` vs `:87-89`; `fallout.rs:145-153` (`.floored()`)
- **Status**: NEW
- **Source**: `:76` ("93.5 (the Pip-Boy truncates the display to 93; the actual value is the float)") vs `:86-88` ("`TotalHP = floor(…)`").
- **Description**: The only `RoundMode::Floor` row rests on a capture that asserts both that the stored value is the float and that it is floored. The difference is 0.5 HP, and only where `L·END` is odd.
- **Suggested Fix**: Reword `:76`, or adjudicate with an xEdit or exe read and keep or drop `.floored()`.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-12`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

