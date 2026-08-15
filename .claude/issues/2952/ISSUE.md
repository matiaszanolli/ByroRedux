# CHAR-D4-05: fnv_faction_thresholds is keyed by REPU FormIDs but named/documented as FACT data — a silent-Neutral cross-keying trap

- **Issue**: [#2952](https://github.com/matiaszanolli/ByroRedux/issues/2952)
- **Finding ID**: `CHAR-D4-05`
- **Labels**: `low,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2952 --json state`.

---

- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: fnv
- **Location**: `crates/core/src/character/reputation.rs:131-137` (`FactionRepThresholds` doc), `:170-172` + `:191-195` (`fnv_faction_thresholds`, `BY_FORM_ID` doc), `:212-213` (`thresholds_for` doc); storage side `crates/core/src/character/components.rs:100-110` (`FactionStanding::faction_form_id`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md:440-442` — "All take `param_1` =
  the `REPU` FormID (`ptReputation` — **reputation is its own REPU record, not the
  FACT faction**)".
- **Description**: Three separate doc comments in `reputation.rs` name the FACT
  faction record as the authoritative source — "vanilla FNV values live on the
  faction record", "the authoritative source remains the parsed faction record",
  "thresholds for a faction by its FalloutNV.esm base FormID" — and the stored key
  is called `faction_form_id`. The keys are in fact **REPU** FormIDs (verified in
  CHAR-D4-04), and `crates/plugin` already parses `REPU` as its own record type
  (`dispatch_misc_gameplay_b.rs:126-133`, `EsmIndex` reputations). The identically
  named `faction_form_id` on `FactionRanks`
  (`crates/core/src/ecs/components/faction_ranks.rs:25`) holds genuine **FACT**
  FormIDs from `NPC_.SNAM` — two different FormID spaces behind one field name in
  the same crate.
- **Evidence**: `condition.rs:700` gets it right ("`param_1` is the global-space
  `REPU` FormID"), which is precisely why the mismatch is invisible: the one live
  caller compensates for prose that would mislead the next one. A future path that
  resolves a faction from `FactionRanks` and passes it to
  `FactionReputation::fame()` / `thresholds_for()` gets `0` / `Range 0` / `Neutral`
  — a silently plausible answer, never an error.
- **Impact**: Latent. `FactionReputation` has no production producer yet, so the
  wrong-space lookup cannot occur today; the cost is a documented invitation to
  wire the wrong record type when it does.
- **Related**: CHAR-D4-04.
- **Suggested Fix**: Rename to `repu_form_id` (or document the key as "the REPU
  record's FormID, not the FACT faction's") across `FactionStanding`,
  `FactionReputation`'s accessors, and `thresholds_for`, and correct the three
  "faction record" provenance sentences to name `REPU`.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
