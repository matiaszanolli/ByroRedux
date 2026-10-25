# TD4-2026-10-25-01: Regression of #6173: Eleven "known-open / do not re-file" callouts in eight skills name issues closed today (fifth recurrence of this class)

Labels: medium, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6353
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: MEDIUM (promoted: two of the sites carry an explicit `do not re-file` directive over code that is now fixed — the #3199 blindfold class `_audit-common.md` forbids; a regression there would be dedup-suppressed)
- **Dimension**: 4 — Audit-Finding Rot
- **Effort**: small
- **Age**: all eight issues closed 2026-10-25 (six by a manual close-sweep at 02:13–02:14Z after `audit-sync` baselined dcad3669d on 10-24; #6089 / #6088 by the HEAD~2 / HEAD~1 fix commits). Not an audit-sync miss — they closed after its baseline.
- **Status**: Regression of #6173 (closed TD4-2026-10-19-01, ~25 such callouts), same class as #5606, #5742, #4369, #5083
- **Sites** (skill:line — claim — truth):
  - `audit-exterior/SKILL.md:33` — "FO3 'Use Land Data' children still draw no ground, open #6089", and the inherit-all "gated by a bare `GameKind::Oblivion` check" — #6089 closed, fixed a9472103b: inheritance now runs for Fallout3NV, pinned by the real-data census. The whole parenthetical is wrong, not just the "open".
  - `audit-fo4/SKILL.md:42` — "Known open: `resolve_precombine_owner` reads the FormID slot byte as a load-order position (#6088)" — closed, fixed 9634aa362: ownership resolves through `load_order_position_of` (`byroredux/src/cell_loader/load_order.rs:363`, used at `precombined.rs:787`); legacy byte read kept only for slot-table-less fixtures.
  - `audit-legacy-compat/SKILL.md:44` — "NiControllerManager / KFM sequence state machine are the known-fragile spots (#4128 open)" — closed, fixed 92173d61f (`NamedSequences` catalogs + ObScript `PlayGroup` consumer).
  - `audit-ui/SKILL.md:46` — "Those runtime-loaded movies still reach Ruffle un-shimmed (#5480, open …)" — closed, fixed 1ac34024f (`load_archive_resource` applies `normalize_scaleform_dialect` once, `crates/ui/src/prepare.rs`).
  - `audit-save/SKILL.md:191` — "`DialogueSpokenInfoForms` — known-open #5459" (bump ledger stops at v32) — closed, fixed a4ccf3b27 (ledger restored + pin `the_bump_ledger_is_contiguous_and_current`).
  - `audit-parsers/SKILL.md:80` — "Known-open gate weakness (dedup, do not re-file): three of the four HKX dimension-gate fixtures leave `mask_size` 0 … (#5446)" — closed, fixed c78e9241e (mutation probe re-run: each clause now reddens exactly its test). **Active suppress-directive over fixed code.**
  - `audit-starfield/SKILL.md:79` — "two docs still say [Skyrim 92 + 16-byte tail], open #5534" — closed, fixed 559a76627 (game-compatibility.md and lighting-from-cells.md rewritten).
  - #5502 ×4: `audit-starfield/SKILL.md:66` ("Known open, don't re-file: the rest of #5502 … awaits a game capture") and `:113` ("rest of #5502"), `audit-nifal/SKILL.md:90` ("undecided half that keeps #5502 open"), `audit-parsers/SKILL.md:87` ("#5502 stays open only for the undecided Starfield glass-coverage signal"). #5502 was closed 2026-10-25 02:14Z "Already fixed": full Parent inheritance landed (5c14ebdf8, 1d7e2c6bd) and the glass-coverage decision is *recorded in nifal.md* (`IsGlass` routes to `bgem_glass` directly; the AlphaBlend forward only passes the classifier's early-return). The skills still call it undecided/needing a capture. **Active suppress-directive at starfield:66.** The still-open sibling is #5509 (additive-family blend modes) — keep that.
- **Suggested Fix**: rewrite each as past tense with the fix SHA (pattern already used elsewhere: "closed, `<sha>`; guard `<test>`"); delete the `do not re-file` phrase at parsers:80 and starfield:66; re-point the glass-coverage wording at `nifal.md`'s recorded decision. Run `_audit-validate.sh` after. Use the sweep script in TD4-03 to confirm zero residue.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD4-2026-10-25-01`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
