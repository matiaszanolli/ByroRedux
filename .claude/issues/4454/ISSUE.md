# D3-02: D3-02: Skyrim NPC pool derivation implements 2 of the capture's 3 composition terms — leveled NPCs get flat pools, and the omission is undocumented in place

- **Labels**: low,character,documentation,game:skyrim
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4454

---

**Source**: charal-skyrim-ruleset.md:603-609 — "NPC Magicka is a 3-part composition … race base (0-200, players start 50) + per-NPC fixed adjustment (−50 to 20000, mostly 0) + 0-10/level from class"; the capture explicitly declines to pursue the full composition as a build target.

**Description**

`derive_skyrim_actor_values` (`crates/plugin/src/esm/records/actor_value_derive.rs:288-313`) resolves Health/Magicka/Stamina from RACE starting value + ACBS offset only. The class term ("0-10/level from class") is uncaptured in detail and uncoded, so a level-40 NPC derives the same pools as a level-1 NPC with the same race and offset. No per-class numbers are captured anywhere, so implementing it now would violate no-guessing — the finding records the frontier, and that neither the profile row comment (`profile.rs:129-132`) nor the derive fn mentions the omitted third term (unlike regen.rs's documented-gap style).

**Evidence**

Derive loop covers only (name, race.starting_X, stats.X_offset) pairs; no level or class input reaches it.

**Impact**

Wrong-by-omission NPC pools on a wired family once leveled NPCs matter to gameplay. No capture-backed number exists to ship today, so LOW.

**Related**

CHARAL audit 2026-09-19 Dimension 5 (population ordering trace).

**Suggested Fix**

A one-line comment on the profile row / derive fn naming the omitted term and its capture line (mirroring regen.rs's documented-gap style), plus a research thread for the per-class pool-growth table (CK class data) before any leveled-NPC gameplay lands.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D3-02, /audit-character 2026-09-19, HEAD `479163836`).*