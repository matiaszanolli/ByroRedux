# CHAR-2026-10-26-L-10: A GECK scope lead for FO3/FNV Action Points exists; the capture and the `player_only()` comment still say none does

- **Severity**: LOW
- **Dimension**: Derived Formulas / Coverage
- **Game**: fo3, fnv
- **Location**: `docs/engine/charal-fnv-fo3-ruleset.md:415-426`; the #2937 comment on the AP row in `fallout.rs`
- **Status**: NEW (relates to closed #2937 and #5240)
- **Source**: GECK *fAVDActionPointsBase* ("base amount of action points available to **an actor**") and *fAVDActionPointsMult* ("how many additional action points **an actor** has"), in `/mnt/data/src/reference/geck-uesp-wiki/(main)/`. Counter-evidence: *Stats Tab - NPC* has no AP entry (#5240). Masters: FNV `65/3`, FO3 `65/2`.
- **Description**: The capture says nothing cited for AP states NPC scope. The wording is weak and not conclusive, so the scope stays `player_only`, but the audit trail should record the lead. The live AP lane does not contradict `.player_only()`: FO3/FNV NPCs carry no AP actor value, so the lane runs for the player only.
- **Suggested Fix**: Add the two citations to the capture row and the `fallout.rs` comment. Keep `player_only` until an engine-side check settles it.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-10`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
