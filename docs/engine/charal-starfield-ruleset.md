# CHARAL — Starfield character ruleset (data capture)

Living capture of the **Starfield** `CharacterRuleset` (CHARAL §5) gameplay
data, per the same LOCKED/PENDING convention as the other per-game docs. No
guessing ([[feedback_no_guessing]]). Parent: [charal.md](charal.md).

**Sourcing note:** UESP does **not** cover Starfield (confirmed 2026-07-04 via
its MediaWiki `siprop=namespaces` API — only Morrowind/Oblivion/Skyrim exist
there). Starfield sources instead from `starfield.fandom.com` (same
curl-`api.php?action=parse` workaround as the Fallout wikis — direct WebFetch
on fandom domains 402s).

## Attributes — LOCKED (none)

Starfield has **no attributes** — same shape as FO4 (perks/skills replace
them). `attributes: []`.

## Skills — LOCKED (roster), PENDING (leveling curve + tier point-thresholds)

Source: *starfield.fandom.com/wiki/Skills*, 2026-07-04. **82 skills across 5
categories, 4 tiers each:**

| Category | Novice | Advanced | Expert | Master | Total |
|---|---|---|---|---|---|
| Physical | 5 | 5 | 3 | 3 | 16 |
| Social | 5 | 5 | 3 | 3 | 16 |
| Combat | 5 | 4\* | 5\* | 3 | 17 |
| Science | 5 | 5 | 3 | 3 | 16 |
| Tech | 5 | 4 | 5 | 3 | 17 |
| **Total** | | | | | **82** |

Full per-tier skill names captured in the fetched wikitext (Physical: Boxing,
Fitness, Stealth, Weight Lifting, Wellness → Energy Weapon Dissipation,
Environmental Conditioning, Gymnastics, Nutrition, Pain Tolerance → Cellular
Regeneration, Decontamination, Martial Arts → Concealment, Neurostrikes,
Rejuvenation; and similarly for Social/Combat/Science/Tech — full roster not
duplicated here, re-fetch the source page if the exact list is needed
verbatim).

\* **Unresolved cross-source discrepancy (2026-07-04):** `starfieldwiki.net`'s
*Starfield:Skills (Combat)* page tiers **Particle Beams as Advanced**, while
`starfield.fandom.com`'s overview page tiers it as **Expert** — both sources
agree the Combat category totals 17 skills and every other skill's tier, only
Particle Beams' placement differs. Not resolved by guessing
([[feedback_no_guessing]]); plausible explanation is a mid-patch tier rebalance
that one wiki updated and the other didn't, but that's speculation, not a
citation. Treat Combat's Advanced/Expert split as `4-or-5 / 5-or-4` until a
third source or a patch-versioned citation breaks the tie.

### Combat category — full per-skill rank effects + challenges (starfieldwiki.net, 2026-07-04)

Confirms the leveling shape from the overview page with concrete data across
all 17 Combat skills: every rank's effect is a **flat percentage bonus with no
attribute scaling at all** (e.g. Ballistics R1/R2/R3 = +10%/+20%/+30% weapon
damage, flat constants — contrast FO4's perk chart, which is SPECIAL-gated;
Starfield has no attributes to gate on, so skill-tree perks are pure
flat-per-rank tables). Challenge progress typically escalates within a skill
as an increasing kill/action count (most commonly `20 → 50 → 125 → 250`, some
skills instead using `20 → 50 → 100` or a damage-dealt total like
`400 → 1,000 → 2,500` for Incapacitation's EM damage) — a design pattern, not
a formula. This confirms the "challenge-gated rank, not a stateless XP curve"
leveling shape already captured above, but the full 17-skill effect/challenge
table is perk-database content (analogous to FO4's 70-cell perk chart) rather
than a CHARAL derivation formula — not transcribed skill-by-skill here to
avoid bloating this doc with content that belongs in a future `Perks` data
table, not the ruleset-formula capture. Re-fetch
`starfieldwiki.net/wiki/Starfield:Skills_(Combat)` (or the per-category
equivalents) if/when Starfield's perk chart is actually implemented.

**Leveling mechanic — structurally unlike every other family so far:**
- Each skill has 4 ranks (1–4). Purchasing a rank costs a Skill Point (from
  character level-ups); advancing to the *next* rank additionally requires
  completing a **per-skill challenge** (e.g. "kill N enemies with melee
  weapons") — challenge progress does not begin until the current rank is
  purchased. This is **neither** Oblivion's skill-use-drives-attribute-bonus
  model **nor** Skyrim's skill-XP-drives-character-level model — it's a third,
  new shape: **level-up grants Skill Points → spent on ranks → gated by
  discrete challenges**, not a continuous XP curve at the skill level at all.
- Higher-tier skills (Advanced/Expert/Master) require a **minimum Skill Point
  investment within the same category** before they unlock for purchase —
  gates by *category* spend, not by an individual skill's own rank (contrast
  Skyrim's Pickpocket/Speech perks, which gate on that *skill's own* level).
  Exact point thresholds per tier are **PENDING** — not given on this page.
- **Background** grants 3 starting Rank-1 skills (confirms the CHARAL §5
  "backgrounds → starting skills" requirement in principle; the actual
  per-background skill triples are on the Backgrounds page, not fetched yet).

**Still PENDING** (needed before a `LevelingModel::STARFIELD` variant or
`SkillSet::STARFIELD` roster can be built per CHARAL §8 item 8):
- The character **XP/level curve** (how much XP per level, i.e. the
  Skill-Point income rate) — not on this page at all.
- Exact category-spend thresholds gating Advanced/Expert/Master tiers.
- Per-skill challenge definitions (out of CHARAL scope regardless — these are
  gameplay triggers, not a character-progression formula, same bucket as
  FO3's companion XP-award threshold).

**Routing:** this data is real and roster-complete enough to eventually
populate `SkillSet::STARFIELD` (82 skills, no governing attribute — Starfield
has none, same as Skyrim's ungoverned roster), but the leveling curve gap
blocks a complete `CharacterRuleset` builder. Not building `starfield.rs`
speculatively ahead of that — same "mechanism only once the data closes"
discipline as everywhere else in CHARAL.

## Companion Affinity — LOCKED (gates), PENDING (reaction deltas) — fourth reputation-family instance

Source: `starfieldwiki.net/wiki/Starfield:Affinity`, 2026-07-04 (fetched by the
user directly — `starfieldwiki.net` is Cloudflare-JS-challenge-blocked for
both `curl` and `WebFetch`, no `api.php` bypass works there unlike the fandom
wikis). Same per-companion `{ score + gate classifier }` shape as FO4's
Affinity (`charal.md` §7.1, `reputation.rs`) — see there for the family
taxonomy this extends to 4 instances.

**Reaction taxonomy** — 5 named categories (`loves / likes / indifferent /
dislikes / hates`) plus a hidden 6th `WantsToTalk` state that always adds
exactly **+1** affinity (used to prompt the companion into a conversation, not
a real approval signal). This is a **different shape** from FO4's Affinity,
which uses 4 reactions (`Liked/Loved/Disliked/Hated`) crossed with a
`Small/Normal/Large` size multiplier (`0.5/1/1.5`) — Starfield instead has
more named categories and no separate size axis (at least none surfaced by
this page).

**PENDING** — the exact point value each of the 5 named reactions adds/removes
is **not given on this page**: the wikitext transcludes a
`{{Conversation Key (affinity)}}` template that would presumably define them,
but the template's expansion wasn't captured in the fetched wikitext. No
guessing ([[feedback_no_guessing]]) — only `WantsToTalk`'s `+1` is a real
number here.

**Possible second axis found — `com_angerlevel`, PENDING decomposition.**
Source: `starfieldwiki.net/wiki/Starfield:Companions`, 2026-07-04, Bugs
section — a debug-command note confirms the actual AV editor IDs:
`com_affinity` (the score documented above) and a **separate**
`com_angerlevel`, both readable/settable via console (`getav`/`setav`). This
page gives no formula for how anger accrues, decays, or interacts with
affinity — only that a companion can become "very angry ... for no clear
reason," which players work around by manually zeroing `com_angerlevel`. If
`com_angerlevel` turns out to be a real second axis (not just a transient flag
Story-Gate-adjacent to affinity), Starfield's companion relationship would be
**2-axis** like FNV's Fame/Infamy, not 1-axis like FO4's Affinity — but that's
speculation pending a source that actually defines the anger mechanic, not a
conclusion. Not changing the "fourth reputation-family instance" classification
above until that source turns up.

**Story Gate — dual-gated progression, genuinely novel vs. every other
reputation-family instance:**
```
Gate N unlocks at:  Affinity ≥ 100·N   (N = 1..8, so 100/200/…/800)
                     AND real-world wall-clock time elapsed ≥ GateMinTime(N)
GateMinTime(1) = 30 minutes;  GateMinTime(2..8) = 1 hour each
```
Each gate unlocks new dialogue/quest content (a per-companion "story"
progression, e.g. Barrett's gate 1 = discussing his deceased husband, gate 5 =
starts his romance quest, gate 6 = commitment). **No other reputation-family
instance found so far gates on real elapsed time** — Karma/FNV
Reputation/FO4 Affinity are all pure in-fiction-state gates. Worth remembering
if a `AffinityStanding`-shaped component is ever built for Starfield: it would
need a wall-clock timestamp field, not just an affinity score, unlike FO4's
equivalent.

The bulk of the source page is ~60 rows of **per-mission dialogue-choice →
reaction-category** mappings (e.g. "player donated credits to X" → `likes` for
one companion, `loves` for another) — this is per-content flavor data, not a
formula or a governance rule, same "too granular, belongs in a quest/dialogue
data table, not a ruleset doc" bucket as Skyrim's Persuasion Options list. Not
transcribed here.

## Crew skills — out of CHARAL scope

The same page lists ~30 "crew skills" (a **different mechanic**: passive
per-crew-member bonuses when assigned to a ship/outpost/as a follower — no
visible rank, effects mostly undocumented even in-game). This is a
party/companion-bonus system, not the Spacefarer's own character progression —
same "real system, wrong layer" routing as FNV's Nerve companion-buff formula.
Not part of `SkillSet::STARFIELD`.
