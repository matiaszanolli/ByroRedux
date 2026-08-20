# #3181 — AUD-2026-08-20-D6-02: the shipped water / underwater audio surface is undocumented in all three status sources

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3181
- **Labels**: `low,tech-debt,documentation`
- **Source report**: `docs/audits/AUDIT_AUDIO_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: LOW
**Dimension**: Manager Lifecycle & ECS/Cell Streaming (documentation)
**Source**: `docs/audits/AUDIT_AUDIO_2026-08-20.md` (`AUD-2026-08-20-D6-02`) — HEAD `bb0b92f2`

## Location

- `crates/audio/src/lib.rs` — module docstring (the phase-by-phase block and its "# Future work" list)
- `crates/audio/src/lib.rs` — `audio_system`'s own numbered docstring
- `docs/feature-matrix.md` — the "Audio (M44 — Phases 1–6 complete)" table
- `ROADMAP.md` — the M44 row

## Description

`/audit-audio` makes the crate docstring a first-class contract: *"If the docstring drifts from the
user-visible API, that's a finding in itself."* Two commits (`948f104a`, `75ad0653`) added public API
and a per-frame pass, and **none of the three authoritative status sources mention any of it**:

1. **`crates/audio/src/lib.rs`** — the phase-by-phase docstring ends at "# Phase 6" and its
   "# Future work" list still reads FOOT / REGN / MUSC / per-cell acoustics + occlusion.
   `AudioWorld::set_underwater` and `AudioWorld::underwater()` are new public methods with no
   module-level coverage. `audio_system`'s numbered docstring lists **three** steps — listener sync,
   dispatch, prune — when the body now runs **five**, including the queue drain (which predates this
   cycle) and `update_underwater_filters`.
2. **`docs/feature-matrix.md`** — the section is still titled *"Audio (M44 — Phases 1–6 complete)"*
   with eight rows, none water-related: no row for underwater filtering, none for water-surface
   one-shots. The skill designates this table as *"the authoritative runtime-status table"*.
3. **`ROADMAP.md`** — the M44 row enumerates Phases 1–6 in detail and stops there. No mention of
   `water_audio_system`, `WaterAudioConfig`, or the submersion low-pass, despite this being the first
   new M44 consumer since Phase 3.5.

## Evidence

`grep -n "underwater\|submerged\|splash\|water" crates/audio/src/lib.rs` finds hits only in the code
body — the constants, `ActiveSound`'s new fields, `AudioWorld`'s new field,
`update_underwater_filters`, and the two filter-construction blocks — and **none** in the module
docstring. `docs/feature-matrix.md`'s M44 table lists eight rows, none water-related.

## Impact

Documentation only, no runtime behaviour. The concrete cost is the next audit cycle or contributor
treating underwater filtering as unimplemented — the docstring's "Future work" section is precisely what
a reader consults for that question — and re-deriving or duplicating it. That is the same trap that
produced ~5 of 30 bad findings in past sweeps, and the reason the skill lists docstring drift as a
reportable defect.

## Suggested Fix

- Add a phase block to the module docstring covering the submersion low-pass (`set_underwater` /
  `underwater()` / `update_underwater_filters` / the two cutoff constants), and move nothing out of
  "Future work" that has not actually shipped.
- Refresh `audio_system`'s numbered step list from three to five.
- Add "Underwater low-pass (submersion-driven)" and "Water-surface splash one-shots" rows to
  `docs/feature-matrix.md`.
- Extend the `ROADMAP.md` M44 row with the water-audio consumer.

## Related

- **#3088** (OPEN) — owns the `ROADMAP.md` M44 **test counts**; do not duplicate that half here. Note
  that its counts have **re-drifted** since the 2026-08-19 refresh: live is **22 default + 6 ignored**
  in `crates/audio/src/tests.rs` (28 `#[test]`, 6 `#[ignore]`) and **11** in
  `byroredux/src/systems/audio.rs`, i.e. **39 audio tests total**, not 37. Posted as a comment on #3088.
- **#3087** (OPEN) — the sibling comment rot in `boot.rs` / `systems/audio.rs`.
- **#1859** / `AUD-2026-07-02-01` — same class (a `SoundCache` docstring citing a pre-Session-34 path).
- **AUD-2026-08-20-D6-01** — the fourth member of what should be one documentation pass.

## Completeness Checks

- [ ] **SIBLING**: all three status sources updated in the same pass — a partial update re-creates the
      contradiction #3088 was filed for
- [ ] **TESTS**: no test applies; the guard here is the audit skill's own docstring contract
