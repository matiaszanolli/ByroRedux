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
| Combat | 5 | 4 | 5 | 3 | 17 |
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

## Crew skills — out of CHARAL scope

The same page lists ~30 "crew skills" (a **different mechanic**: passive
per-crew-member bonuses when assigned to a ship/outpost/as a follower — no
visible rank, effects mostly undocumented even in-game). This is a
party/companion-bonus system, not the Spacefarer's own character progression —
same "real system, wrong layer" routing as FNV's Nerve companion-buff formula.
Not part of `SkillSet::STARFIELD`.
