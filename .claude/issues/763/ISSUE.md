# #763: SF-D6-04: Starfield ESM smoke-test binary — `--sf-smoke <CELL>` to measure FO4-dispatch resolve rate before committing to SF ESM parser

URL: https://github.com/matiaszanolli/ByroRedux/issues/763
Labels: enhancement, low, legacy-compat

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 6, SF-D6-04)
**Severity**: LOW (forward-blocker / planning)
**Status**: NEW (recommendation tracker)

## Description

Starfield ESM is currently dispatched under `GameKind::Starfield` via the FO4 record-parser path — never validated. The `crates/plugin/src/legacy/` directory has `tes3.rs`, `tes4.rs`, `tes5.rs`, `fo4.rs` but **no `starfield.rs`**. Several Starfield-new record types have no constants nor parsers: `PNDT` (Planet Data), `STDT` (Star Data), `BIOM` (Biome), `SFBK` (BookSet), `SUNP` (Sun), `GBFM` (Generic Base Form Module), `GBFT`. Several FO4 records evolved with new sub-records: `STAT`, `CELL`, `REFR`, `LIGH`, `DOOR`, `MSTT`, `LGTM`.

Before committing to a full SF ESM parser, we need to measure: of REFRs in a Starfield interior cell, what % have a base form resolvable via FO4 dispatch? The answer is the difference between "FO4 works for 80% of records, write 20% of new parsing" and "completely different schema, write everything from scratch."

## Recommended deliverable

A `cargo run -- --sf-smoke <interior CELL EDID>` smoke-test path that:

1. Walks `Starfield.esm` under the existing `GameKind::Starfield` (FO4-dispatched) path.
2. Picks the smallest interior cell (typically a training room or debug cell).
3. Reports per-record-type the percentage of REFRs whose base form is resolvable.
4. Logs which sub-record types of evolved records (`CELL`, `REFR`, etc.) are silently dropped.

Output target: a single percentage + a list of unhandled sub-records. That tells us whether SF ESM is "FO4 works for 80%" or "completely different" — without it, milestone B sizing is guessing.

## Reference (Apr 2026)

- **SF1Edit** (xEdit fork) — active, 4.1.5o (2025-10-01). De-facto reference; "spec" lives in `.pas` definition files.
- **niftools/nifskope** #232 — partial NIF, no ESM.
- **Wrye Bash** #667 — open since 2023, no merged support.

No human-readable record-by-record spec analogous to UESP for TES4/TES5.

## Completeness Checks

- [ ] **TESTS**: The smoke binary is the test.
- [ ] **SIBLING**: ROADMAP must document the smoke-test delivery as a gate before Milestone B.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- ROADMAP Milestone B ("Starfield interior cell renders") is gated on the smoke-test outcome.
- `LegacyFormId::is_esh()` / `esh_index()` / `esh_local()` already plumbed at `crates/plugin/src/legacy/mod.rs:89-104` (Medium Master slot 0xFD) — preparatory work that this smoke-test exercises.
