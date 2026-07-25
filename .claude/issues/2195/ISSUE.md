# SKY-D1-NEW1-01: audit-skyrim SKILL.md cites stale walker.rs location for alpha-cascade gate sites

**GitHub Issue**: [#2195](https://github.com/matiaszanolli/ByroRedux/issues/2195)
**Severity**: LOW
**Labels**: low, nif-parser, documentation
**Source**: `docs/audits/AUDIT_SKYRIM_2026-07-25.md`, finding LOW-1

## Summary
`.claude/commands/audit-skyrim/SKILL.md`'s Dimension 1 checklist says the two
`!info.alpha_property_consumed` gate sites are "consulted at the two gate sites
in `crates/nif/src/import/material/walker.rs`". A module split moved this
logic into `crates/nif/src/import/material/dedicated_shader.rs` (Skyrim+
dedicated-ref implicit-blend write, line 488) and
`crates/nif/src/import/material/legacy_properties.rs` (legacy
`NiAlphaProperty` cascade, line 65). `walker.rs` no longer contains either
gate — only a stale comment referencing the field
(`crates/nif/src/import/material/walker.rs:122`).

The underlying logic itself is verified correct (14/14 `alpha_flag_tests.rs`
pass); only the audit-skill's path reference is stale.

## Evidence
```
$ grep -n "alpha_property_consumed" crates/nif/src/import/material/walker.rs
122:    // implicit-blend gate (#1202) can consult `alpha_property_consumed`.

$ grep -n "alpha_property_consumed" crates/nif/src/import/material/dedicated_shader.rs
488:        if !info.alpha_property_consumed {

$ grep -n "alpha_property_consumed" crates/nif/src/import/material/legacy_properties.rs
65:    if !info.alpha_property_consumed {
```

## Impact
A future audit following the skill's literal instructions would search the
wrong file and could wrongly conclude the guard regressed. No runtime/parse
impact — this is audit-infrastructure hygiene only.

## Suggested Fix
Update the Dimension 1 entry-point list in
`.claude/commands/audit-skyrim/SKILL.md` to cite `dedicated_shader.rs` and
`legacy_properties.rs` instead of `walker.rs`, matching the path-reference
convention in `_audit-common.md`.

## Dedup Check
No matching open or closed issue found. #2190 (`SCR-D7-NEW4-02`) is the same
*shape* of finding (stale entry-point path in an audit SKILL.md after a
module split) but in `audit-scripting`, a different subsystem — not a
duplicate.

## Path-Validation Gate Note
`.claude/commands/_audit-validate.sh` was run before processing this finding
per the skill's step 2. It reported 9 STALE path refs, all in *other*
SKILL.md files (`audit-ecs`, `audit-fnv`, `audit-incremental`,
`audit-oblivion`, `audit-tech-debt`, `audit-starfield`,
`.claude/commands/_audit-common.md`) referencing `ai.rs`, `actor.rs`, and
`shader_tests.rs` — none of which are this report's finding. The gate does
not flag `walker.rs` itself (the file still exists; only the *logic inside
it* moved, which a path-existence check cannot detect) — that's exactly why
this required a manual audit finding rather than being caught automatically.
Those 9 unrelated stale refs are out of scope for this publish run (likely
surfaced by other concurrent `/audit-publish` runs against other reports in
this session) and were not filed as new issues here.
