# Issues 4068, 4072, 4073, 4084

All four from `docs/audits/AUDIT_ESM_2026-09-09.md` (filed by `/audit-publish`).

## #4068 — ESM-2026-09-09-D4-01 (MEDIUM, Record Schema Dispatch)
`#3616` converted `InfoRecord` response handling to push per-segment
`ResponseSegment`s, keyed on a fresh `TRDT` sub-record opening each segment
(`crates/plugin/src/esm/records/misc/dialogue.rs`). FO4 and Starfield rename
the per-segment opener to `TRDA` (20 bytes on FO4, 12 bytes on Starfield) —
no arm anywhere recognizes it, so `NAM1`/`NAM2` (which write into
`current_response.get_or_insert_with(Default::default)`) keep assigning into
the SAME lazily-created segment across the whole record, exactly the
pre-#3616 collapse-to-one-segment bug, still live for two titles. Census:
FO4 74,996 `TRDA` records / 4,988 multi-segment INFOs / 7,288 lost segments;
Starfield 130,284 `TRDA` / 11,564 multi-segment / 16,911 lost — 24,199 total
discarded response segments (~3x #3616's own count).
Fix (step 1 only — layout-free, zero-risk, per the issue's own two-step
plan): add `TRDA` to the segment-opening arm (same `current_response.take()`
→ push, start fresh) so it behaves identically to `TRDT` for the purpose of
NOT losing segments. Do NOT decode `TRDA`'s emotion/response-number payload
layout — no cited source for either games' byte layout, guessing forbidden
per `feedback_no_guessing`.

## #4072 — ESM-2026-09-09-D6-07 (MEDIUM, Localized Strings)
lstring id `0` is the authored "no string" convention (never a valid id in
any companion table — `skyrim_english`/`fallout4_en` both start at `0x1`),
but `read_lstring_or_zstring` (`crates/plugin/src/esm/records/common.rs`)
doesn't special-case it before the table lookup, so it synthesizes the
placeholder text `<lstring 0x00000000>` — indistinguishable from a genuine
unresolved id, and poisons `.is_empty()` checks. 10,889 fields on
`Skyrim.esm`, 8,030 on `Fallout4.esm` (515 confirmed-reachable on Skyrim
alone, 100% of them `CLAS/DESC`).
Fix: return `String::new()` for `id == 0` before the table lookup, mirroring
`remap_fid`'s existing `if raw == 0 { return 0; }` null convention. Pin with
a unit test alongside the existing `read_lstring_or_zstring_*` tests.

## #4073 — ESM-2026-09-09-D6-02 (MEDIUM, Localized Strings)
A whole-game localization failure (all three string tables — strings/
dlstrings/ilstrings — fail to resolve) produces zero diagnostic at any log
level: `load_file`'s absent-file arm is silent (only a parse-failure warns),
and `install_strings_guard` installs the empty `StringTableSet`
unconditionally. This is what makes #4072-class bugs invisible in practice.
Fix: in `install_strings_guard` (`byroredux/src/cell_loader/load_order.rs`),
emit one `log::warn!` per plugin when all three fields of the loaded
`StringTableSet` are `None` — naming the plugin and language tried. One line
per plugin, no per-form spam.

## #4084 — ESM-2026-09-09-D7-09 (LOW, test-gap, ESM→ECS Handoff)
`every_index_map_is_a_category_or_a_recorded_exclusion`
(`crates/plugin/src/esm/records/index.rs`) — the guard that makes "a
category silently dropped" structurally impossible — extracts fields via
`rest.split_once(": HashMap<")`, so any non-`HashMap` collection
(`HashSet`, `BTreeMap`, `Vec`, a newtype) is invisible to the scan. Both of
`EsmIndex`'s existing non-HashMap collections
(`deleted_record_metadata: HashSet<u32>`,
`skipped_unconsumed_groups: Vec<[u8; 4]>`) already rely on being
hand-written into `merge_from` with no guard coverage — latent today (every
category is a HashMap) but the guard's own stated contract is violated.
Fix: widen the scan to `pub <ident>: <Type><`, and require every such field
to be in `categories()`, in `EXCLUSIONS`, or in a new `MANUALLY_MERGED` list
naming the `merge_from` line that handles it (documenting the two existing
cases as deliberate).

## Domain
All `byroredux-plugin` (`crates/plugin`), except #4073 whose fix site
(`install_strings_guard`) is in the binary crate `byroredux`
(`byroredux/src/cell_loader/load_order.rs`).
