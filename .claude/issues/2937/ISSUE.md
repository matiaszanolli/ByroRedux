# CHAR-D2-03: FO3/FNV Action Points is tagged player_only with no capture-document support

- **Issue**: [#2937](https://github.com/matiaszanolli/ByroRedux/issues/2937)
- **Finding ID**: `CHAR-D2-03`
- **Labels**: `low,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2937 --json state`.

---

- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: fo3, fnv
- **Location**: `crates/core/src/character/fallout.rs` (`fallout3_ruleset`, `falloutnv_ruleset` — the `ActionPoints` rows)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md` § Derived statistics — the table annotates scope explicitly on every other row (Health *"**LOCKED** (player)"*, Carry Weight *"**LOCKED** (actor-general)"*, Radiation/Poison Resistance *"**LOCKED** (actor-general…)"*) and gives Action Points **no scope annotation at all**: *"| Action Points | AGI | `65 + 2·AGI` (cap 85) | `65 + 3·AGI` (cap 95) | **LOCKED** |"*. The same document's Carry Weight § states the discriminating rule: *"The `fAVD…` (Actor Value Derived) prefix means this derives the … AV for **any** actor"* — and its Action Points § names *"the same `fAVDActionPoints{Base,Mult}` GMST family"*.
- **Description**: The numbers are right (rows 6 and 8 above); the **scope tag** is an engine decision no capture line backs. By the document's stated `fAVD` heuristic, `fAVDActionPointsBase/Mult` would make AP actor-general for FO3/FNV, the same way it makes Carry Weight actor-general. The FO4 row *is* sourced — FO4 NPCs read a baked `DNAM` "Calculated Action Points" — but that evidence is FO4-specific (`PRPS`/`DNAM` are an FO4-era layout; the FO3/FNV § "NPC stat storage" note says only that auto-calc-OFF NPCs store explicit skill/SPECIAL values, saying nothing about AP). `fallout.rs`'s module docstring generalises FO4's justification (*"NPCs ship baked values or derive them differently"*) across all three games without a citation for two of them.
- **Evidence**: `fallout3_ruleset`: `DerivedStatFormula::affine(av(a), 2.0, 65.0).capped(85.0).player_only()`; `falloutnv_ruleset`: the `3.0/95.0` twin. Compare the Carry Weight row two functions away, deliberately left `ActorGeneral` on the strength of the `fAVD` rule.
- **Impact**: Conservative direction — `GetActorValue(ActionPoints)` on an FNV NPC without the AV returns the absent default `0.0` rather than a possibly-correct `65 + 3·AGI`. Nothing is over-computed, so no stat is inflated; the cost is a silently-missing derivation and an unsourced constant sitting in a table whose whole premise is that every entry is sourced. It also makes the FO3↔FNV *player* Health/AP deferral look wider than it is.
- **Related**: The known-open FO3↔FNV player Health/AP divergence (deliberately deferred — **not** re-filed here; this finding is about the *scope tag*, not the divergence).
- **Suggested Fix**: Either cite a line making FO3/FNV AP player-only and add it to the capture document, or flip the two rows to `ActorGeneral` per the `fAVD` rule; in the meantime annotate the code with "scope unsourced, chosen conservatively" so it is not mistaken for a captured fact.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
