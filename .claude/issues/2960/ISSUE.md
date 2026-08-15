# CHAR-D6-03: charal.md still reads Status: PROPOSED while all four sibling layer docs read ACTIVE

- **Issue**: [#2960](https://github.com/matiaszanolli/ByroRedux/issues/2960)
- **Finding ID**: `CHAR-D6-03`
- **Labels**: `low,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2960 --json state`.

---

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: FO76 (status half: all)
- **Location**: `docs/engine/charal.md` (Status header, line ~19; §8 rollout list)
- **Status**: NEW
- **Source**: `docs/engine/charal-fo76-ruleset.md` — "## Leveling — LOCKED …
  `XP_to_next(L) = 160·L − 120`"; the derived table rows `Carry Weight … 150 + 5·STR
  **LOCKED**`, `Health … 250 + 5·END (no level term) **LOCKED**`, `Action Points …
  60 + 10·AGI **LOCKED** (matches FO4)`
- **Description**: Two related drifts in the same document.
  **(a) Status.** `charal.md` is the only one of the five abstraction-layer specs
  still marked `PROPOSED`: `nifal.md` reads `ACTIVE (opened 2026-05-28)`, `exal.md`
  `ACTIVE`, `physal.md` `ACTIVE (opened 2026-06-14)`, `watal.md`
  `ACTIVE (design 2026-06-19; implementation checkpoint 2026-08-10)`. CHARAL ships 13
  sub-modules, five ruleset builders, two wired games, a registered scheduler system
  and 94 green tests. A reader applying the sibling convention concludes CHARAL is
  unbuilt design.
  **(b) FO76's absence from §8.** The rollout order runs items 1–8 and never mentions
  FO76. Every ruleset row FO76 needs is **LOCKED** in its capture, and every shape it
  needs already exists in code: `AttributeSet::FALLOUT` (the capture states FO76 needs
  no changes to it), `SkillSet::NONE` (no skills), `LevelingModel::XpCurve` (fits
  `160·L − 120` exactly), and `LevelReward::SpecialOrPerk` — whose own docstring
  already claims "**FO4 / FO76**". Three of its four derived stats are clean affines
  the existing `DerivedStatFormula::affine` expresses directly. The one genuinely open
  modelling question is FO76's weapon-type-split Melee Damage (`STR/20` for 1H/2H vs
  `STR/10` unarmed), which a single-row table cannot hold both halves of.
  The capture does carry a self-note ("Not yet in the CHARAL §8 rollout order"), so
  this is not fully silent — but that note lives in the child document and explains
  *why FO76 can't just reuse FO4*, not *why it isn't built*. The document that owns
  the rollout is silent, so nothing on the planning path records that the cheapest
  remaining game is buildable today.
- **Evidence**: `grep "^\*\*Status\*\*" docs/engine/{nifal,exal,physal,watal,charal}.md`
  → four `ACTIVE`, one `PROPOSED (design, 2026-06-29)`. `grep -n "pub const \(FO3\|FNV\|FO4\|OBLIVION\|SKYRIM\|FO76\|STARFIELD\)" crates/core/src/character/leveling.rs`
  → five consts, no `FO76`. `LevelReward::SpecialOrPerk`'s docstring: "FO4 / FO76: one
  point per level…".
- **Impact**: Planning-surface drift. The status line understates a shipped layer to
  every reader who compares it against its four siblings; the §8 omission hides the
  one remaining game whose data is closed. Neither has runtime effect.
- **Related**: `CHAR-D6-02` (same document, content staleness); `CHAR-D6-04`.
- **Suggested Fix**: Move the status to `ACTIVE` with an implementation-checkpoint
  date, matching `watal.md`'s form. Add an FO76 item to §8 stating the four LOCKED
  formulas, that the leveling/reward shapes already exist, and that split Melee Damage
  is the single open modelling question.

---

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
