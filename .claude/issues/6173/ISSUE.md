# TD4-2026-10-19-01: ~25 "known-open / do not re-file" callouts in 14 skills name issues that are now CLOSED (and several premises are fixed in the tree)

Labels: medium,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: MEDIUM (a "do not re-file" on a closed issue suppresses a regression finding; severity-table row "stale audit baseline that misleads")
- **Dimension**: 4 | **Effort**: small (one sweep) | **Age**: closures 2026-10-15 .. 2026-10-19, after the sync
- **Locations** (callout line -> issue, closing commit; all `gh` state CLOSED/completed unless noted):
  - `audit-nifal/SKILL.md:90` and `audit-skyrim/SKILL.md:61` #4256 (`2266f7778`, `Material::source_shader_type` now crosses the boundary)
  - `audit-audio/SKILL.md:19` #4743 (`519bb02c4`); `:18` "Oblivion voice (#5526 open)" (`fa7591ca2`, 10-16); `audit-oblivion/SKILL.md:28` #5526, #5505 (`75b921cf4`); `audit-exterior/SKILL.md:33` #5505
  - `audit-fo3/SKILL.md:29` #4772 (`fc9622f6f`)
  - #5015 closed 10-19 after `e1bd40bcf` "decode FO4 skeleton ragdolls and pose human/feral-ghoul corpses from XRGD": `audit-fnv/SKILL.md:64`, `audit-fo4/SKILL.md:92` ("FO4 corpses are still unposed (open #5015)" is now false), `audit-skyrim/SKILL.md:88`, `audit-physics/SKILL.md:80`
  - `audit-ecs/SKILL.md:37` #5032, `:127` #5713, `:253` #5714
  - `audit-fo4/SKILL.md:89` #5523; `audit-starfield/SKILL.md:77` #5533 (`bfea48f82`); `audit-parsers/SKILL.md:87` and `audit-nifal/SKILL.md:165` #5525
  - `audit-parsers/SKILL.md:61` #5009 ("`debug_load.rs`'s `resolve_nif_bytes` keeps one"): fixed `44219e881` (10-15); `resolve_nif_bytes` no longer exists in the tree
  - `audit-esm/SKILL.md:287` "instance-value reader is open tracker #5627" (`2c203278f` decodes WTHS instance values, 10-19); `:427` "#5609 (open)" (`490e76397`, 10-15)
  - `audit-scripting/SKILL.md:70-79` block headed "Known-open - cite, don't re-file (verified 2026-10-15)" lists #5682, #5752, #5787-#5790 (and #5734 as the live cause), all CLOSED; `:668` "#5790 is open ... #5789 on `%0.f`" (both closed 10-17)
  - Premise half-fixed, issue still open: #5502's "remaining half" (texture-set slot / flat-colour Parent inheritance) was fixed 10-19 by `1d7e2c6bd` ("no texture 2,049 -> 217"; the issue stays open only for the glass-coverage decision). Still written as known-open with the old 2,051/10,158 figure at `audit-parsers/SKILL.md:87`, `audit-nifal/SKILL.md:90,165`, `audit-starfield/SKILL.md:66,111`.
- **Evidence**: batch state from `gh issue list --state all --limit 9000` (6026 issues) joined against every `#N` within an open/known/still/Existing clause in `.claude/commands/` (126 candidates); each line above re-read. Verified correctly OPEN (kept): #3307, #4113, #4115, #4137, #4232, #5365, #5446, #5459, #5534, #5646, #5647, #5750, #5764, #5778.
- **Proposed fix**: rerun `/audit-sync` (the closed-issue sweep is its first step); for each line either move the issue to the "Fixed ... verify, don't re-file" list or rewrite as a regression guard. Treat #5015's FO4 and #5502's slot-inheritance claims as behaviour changes needing a re-read of the code, not just a relabel.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
