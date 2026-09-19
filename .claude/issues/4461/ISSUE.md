# D6-05: D6-05: ROADMAP known-issues bullet stale — "only FO4 and FNV reach an actor" / Skyrim "no construction site" / #2941 / boot.rs

- **Labels**: low,character,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4461

---

**Source**: `docs/feature-matrix.md:259-266` (current, accurate); `crates/core/src/character/profile.rs` builder arms.

**Description**

The unchecked known-issues bullet at `ROADMAP.md:1227` ("~60 % of shipped CHARAL is runtime-dead", Session-67 dated) asserts: only FO4 and FNV reach an actor; Oblivion AND Skyrim fully assembled with no construction site; FO3's ruleset shadowed by FNV's (#2941); regen registered in `boot.rs`. Four claims are now false: #2941 CLOSED (`b434e4c0` profile-centralization; FO3 builds its own model, pinned by `fo3_and_fnv_profiles_build_their_own_distinct_leveling_model`), #3848 CLOSED (Skyrim wired — four games now reach an actor), and `boot.rs` is the dead-path class #4109 swept out of four other files (this copy sat in ROADMAP). The still-true parts (regen config never inserted; affliction never registered; FO76 no builder) are why the bullet stays `[ ]`.

**Evidence**

profile.rs arms + feature-matrix.md:261 vs ROADMAP.md:1227 text.

**Impact**

The ROADMAP's authoritative known-issues list overstates the wiring gap by two games; a reader would re-do closed work or distrust the feature matrix.

**Related**

#2941, #3848, #4109, #2932-#2962.

**Suggested Fix**

Update the bullet in place with a closure annotation: four games reach an actor; FO3 shadowing closed by #2941; Skyrim wired by #3848; repoint boot.rs → boot/schedule/update.rs; the surviving gap is Oblivion (#3768) + regen config + affliction + FO76.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D6-05, /audit-character 2026-09-19, HEAD `479163836`).*