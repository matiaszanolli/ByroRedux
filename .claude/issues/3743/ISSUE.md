# #3743 — TD3-2026-08-30-02: the ACTI record's "Runtime consumer gap (M47.0)" doc block is half-true — `script_form_id` has been live since M47.0

**Labels**: documentation, low, tech-debt, esm-plugin, doc-rot

---

- **Severity**: LOW
- **Dimension**: 3 — Documentation Rot
- **Location**: `crates/plugin/src/esm/records/misc/world.rs` — the "Runtime consumer gap (M47.0)" doc block above `ActiRecord`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD3-2026-08-30-02`), HEAD `64f64480`

## Description

The doc block reads:

> **Runtime consumer gap (M47.0):** the captured `script_form_id` / `sound_form_id` /
> `radio_form_id` cross-refs **ride through unused today**; the trigger / event-hook
> runtime **planned for M47.0** will dispatch ActivateEvent to the SCRI-linked script …
> Until then the stub closes the parser-side silent drop so the M47.0 work has one grep
> target.

`script_form_id` **is** consumed. `ActiRecord` is the first arm of
`EsmIndex::base_record_script` (`crates/plugin/src/esm/records/index.rs`, `fn
base_record_script`), which `byroredux/src/cell_loader/references/attach.rs` calls to
resolve `index.scripts.get(&script_form_id)` — the attach path even logs with an
`"M47.0: "` prefix, i.e. M47.0 shipped. The field-level doc at `ActiRecord.script_form_id`
("Referenced by trigger-system dispatch **once it lands**") is the same drift restated.

`sound_form_id` and `radio_form_id` **are** still unconsumed (verified: no reader outside
`records/`), so the paragraph is **half-true — which is worse than wholly stale**, because
a reader cannot tell which half to trust.

## Suggested Fix

Split the paragraph: say `script_form_id` is live via `base_record_script` since M47.0,
and keep the deferral note for the two sound fields only. Update the field-level doc on
`ActiRecord.script_form_id` in the same change. Effort: trivial.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — `world.rs` carries a second "Runtime consumer gap (M47.0)" block (the menu tree / password fields); verify whether it is also half-true
