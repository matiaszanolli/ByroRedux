# TD4-2026-10-25-05: `audit-tech-debt` Phase-1 paragraph: headline ">2000 production LOC: 4" and the watch-list are now wrong

Labels: low, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6385
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 4 · **Effort**: trivial
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:89-103`
- **Status**: recurrence of #6176 (closed TD4-2026-10-19-04); not a regression of a fix — the paragraph is dated "diff direction only" and rots each audit by design
- **Evidence** (`/tmp/audit/tech-debt/baseline.txt`, `dim1_big.txt`; prod_loc self-test ok): files >2000 prod LOC = **5**, not 4. New entry `crates/scripting/src/obscript_effects.rs` 2034 (1528 at dcad3669d → 2032 at d5d13edcc, #6139 stage 8, 2026-10-24; wc 2951). Other drift: `shader_constants_data.rs` 2964→2972, `byroredux/src/npc_spawn.rs` 1846→1911, `context/init.rs` 1920→1950; `#[ignore]` 368→373, `allow(dead_code)` 43→41 (markers 26 and test-heavy 93 unchanged). Items that checked out: audio 2086, types.rs 2011, mq101_conformance 2157, inventory 1956, panels 1938, gpu_timers 1929, world.rs 1912, merge.rs 1860; hub sizes "~70–150 prod LOC" (68–153 measured) and "largest child ~1400" (interp.rs 1396); "24 crate dirs and three of four tools unnamed" (recipe re-run: exactly 24 / 3); no new crate/tool manifests since 10-21; `cell_loader/exterior.rs` split (1d383cc5a) is already reflected in `_audit-common.md:9`, `_audit-owners.md:147` and four skills.
- **Suggested Fix**: re-baseline the paragraph (5 files; add obscript_effects.rs to the list with its #6139 cause; move npc_spawn.rs/init.rs/inventory.rs/panels.rs within-5% rows), or — better — replace the instance list with "run the recipe" and keep only the count delta, since the list is regenerated every audit anyway.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD4-2026-10-25-05`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
