# SCR-D6-NEW2-01: Fragment-dispatch docs describe the now-live QUST-fragment pipeline as an unwired no-op

**Labels**: low, documentation

**Severity**: LOW
**Dimension**: Scripting Runtime Systems
**Untrusted-Input**: No
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-07-16.md`

## Location
`crates/scripting/src/fragment.rs:11-26` (module doc, "Population (pending)"); `byroredux/src/boot.rs:604-607` ("no-op until the QUST-VMAD fragment decoder lands, #1739"); `crates/plugin/src/esm/records/script_instance.rs:46` ("fragment decode is a later phase")

## Description
`8a70b81a` wired `populate_quest_fragments_from_pex`/`_from_script` into the cell loader and is validated end-to-end on real Skyrim data (845 scripted quests → 742 lowered fragments). The commit updated `translate/mod.rs` and the design doc to say "landed," but missed `fragment.rs`'s own module doc, and two more sites (`boot.rs`'s scheduler comment, `script_instance.rs`'s module doc) were never on the update list and are stale for the same reason.

Verified current: `crates/scripting/src/fragment.rs`'s module doc still reads "Population (pending): ... the resource stays empty at runtime and the dispatcher is a no-op"; `byroredux/src/boot.rs` still has the "no-op until the QUST-VMAD fragment decoder lands, #1739" comment; `script_instance.rs:46` still says "fragment decode is a later phase" — while `populate_quest_fragments_from_pex` is in fact called live from `byroredux/src/asset_provider/script.rs`.

## Impact
Purely informational — no behavior is wrong. But a future contributor reading `fragment.rs`'s header (the natural first stop before touching `quest_fragment_dispatch_system`) would reasonably conclude the dispatcher is dead code and skip regression-testing it.

## Related
Not a re-file of #1907 (already fixed) — purely the three doc sites lagging the commit (`8a70b81a`) that fixed it.

## Suggested Fix
Update `fragment.rs:11-26` to "Population (shipped, #1739/8a70b81a)"; drop or update the `boot.rs` "no-op until … lands" comment; update `script_instance.rs:46` to point at `parse_quest_fragments` in the same file.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only fix)
