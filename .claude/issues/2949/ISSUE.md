# CHAR-D4-02: the 4x4 ReputationStanding sentiment bucketing is unsourced and its guard test restates the code

- **Issue**: [#2949](https://github.com/matiaszanolli/ByroRedux/issues/2949)
- **Finding ID**: `CHAR-D4-02`
- **Labels**: `low,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2949 --json state`.

---

- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: fnv
- **Location**: `crates/core/src/character/reputation.rs:362-375` (`ReputationSentiment`), `:433-447` (`ReputationStanding::sentiment`), test `standing_sentiment_matches_grid_colours` (`:560-588`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md:486-487` — the grid is captured as
  "16 standing titles (shared across all factions; positive=green, mixed=black,
  negative=red)" followed by the title table at `:489-494`. The document records the
  **titles** and the colour **legend**; it never records which title is which colour.
- **Description**: `sentiment()` assigns all 16 cells to three buckets (5 Positive /
  5 Negative / 6 Mixed). That mapping has no capture-document backing. It is
  *internally* plausible — green ⇔ fame-range > infamy-range with infamy ≤ 1, red the
  mirror image, black the diagonal — but it is not derivable from the doc, and the
  two cells that break the "higher axis wins" rule (`DarkHero` at fame 3/infamy 2 and
  `SoftHeartedDevil` at fame 2/infamy 3 are both Mixed, not Positive/Negative) are
  exactly where a from-memory transcription would go wrong without anyone noticing.
- **Evidence**: the test that is supposed to guard this is
  `standing_sentiment_matches_grid_colours`, which iterates three hardcoded lists and
  asserts `s.sentiment()` equals the bucket the same source file just assigned — it
  can only fail if someone edits one of the two lists and not the other. It cannot
  detect a mis-transcribed colour, despite its name claiming fidelity to "grid
  colours".
- **Impact**: Consumers reading `sentiment()` (the intended shape is dialogue/vendor
  hostility gating) would get a plausible-looking wrong answer for the two ambiguous
  cells. Latent — no caller today.
- **Related**: CHAR-D4-01, CHAR-D4-04 (both are unsourced values in the same file).
- **Suggested Fix**: Add the per-cell colour to
  `charal-fnv-fo3-ruleset.md`'s grid (the source page renders them), then cite it
  from the `sentiment()` doc comment; or drop `sentiment()` until a caller needs it,
  rather than shipping an unsourced classifier under a test that reads as verified.

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
