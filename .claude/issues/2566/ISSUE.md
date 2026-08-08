# OBL-D1-05: audit-oblivion SKILL.md mis-states the Oblivion retail version and the pre-Gamebryo fallback behaviour

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2566
**Finding ID**: OBL-D1-05

**Severity**: LOW
**Dimension**: NIF Version Handling
**Location**: `.claude/commands/audit-oblivion/SKILL.md:22-24,68-70`
**Status**: NEW

## Description
The skill's own brief names v20.0.0.5 as the dominant retail body; the live census says it's actually v20.0.0.4 (7,282 files vs. 1,680) — `version.rs` already documents this correctly, the skill contradicts it. The skill also claims pre-v3.3.0.13 files return an empty `NifScene`; the parser actually parses inline and only fails (with a `warn`) on a mid-file inline-name read error.

## Evidence
Confirmed directly: `SKILL.md:22-24` names v20.0.0.5 as "what most clutter / architecture / creature meshes are."

## Impact
Documentation-only, but it is the brief every Dimension-1 agent reads first — feeds incorrect framing into every future Oblivion audit cycle.

## Related
#2348/#2347 (same doc-drift class from earlier Oblivion audits).

## Suggested Fix
Correct the skill to name v20.0.0.4 as dominant (matching `version.rs`'s live census), and correct the pre-v3.3.0.13 fallback description to match the parser's actual behavior (inline parse + warn-on-error, not an empty-scene return).

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
