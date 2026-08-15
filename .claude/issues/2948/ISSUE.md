# CHAR-D4-01: AffinityBand::Idolize.name() returns "Infatuation", a string in no capture document

- **Issue**: [#2948](https://github.com/matiaszanolli/ByroRedux/issues/2948)
- **Finding ID**: `CHAR-D4-01`
- **Labels**: `low,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2948 --json state`.

---

- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: fo4
- **Location**: `crates/core/src/character/reputation.rs:252-265` (`AffinityBand::name`)
- **Status**: NEW
- **Source**: `docs/engine/charal.md:425-426` — "7 bands (Hatred/Disdain/Neutral/Friend/Admiration/**Confidant/Idolize**) at thresholds `-500/0/250/500/750/1000`".
- **Description**: Six of the seven `AffinityBand` variants return exactly the name
  the capture document records. The seventh returns `"Infatuation"`, a string that
  appears in **no** CHARAL capture document (`grep -rn "Infatuation" docs/` → no
  hits; the only near-match anywhere in the corpus is FNV Reputation's unrelated
  `Idolized` grid cell). The method's own doc comment asserts provenance it does not
  have — "The wiki's relationship name for this band."
- **Evidence**:
  ```rust
  AffinityBand::Confidant => "Confidant",
  AffinityBand::Idolize => "Infatuation",   // enum says Idolize, name says Infatuation
  ```
  No test asserts any `AffinityBand::name()` value — `affinity_bands_at_exact_boundaries`
  and `affinity_band_is_ordered_and_one_byte` cover thresholds and layout only, so
  nothing pins the string.
- **Impact**: The max-affinity band is the one that gates the companion perk, so it
  is the band most likely to be surfaced in UI or a quest condition. Any consumer
  displaying or string-matching `.name()` gets a label that disagrees with both the
  enum variant and the capture document. No gameplay path today (no caller).
- **Related**: CHAR-D4-02 (the other unsourced classifier string set in this file).
- **Suggested Fix**: Either rename the returned string to `"Idolize"`, or — if
  "Infatuation" is the real FO4 in-game label — add the citation to
  `charal.md` §7.1 and rename the variant to match. Pin whichever wins with a
  `name()` test, as `KarmaBand`/`ReputationStanding` effectively have via their
  assert messages.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
