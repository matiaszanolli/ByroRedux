# #3723 — ESM-2026-08-30-D2-02: Skyrim AMMO.DATA is 20 bytes on disk, not the 16 the decoder's comment claims

**Severity**: LOW · **Location**: `crates/plugin/src/esm/records/items.rs::parse_ammo`
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D2-02)

The `Skyrim | Fallout76 | Starfield` AMMO DATA arm's own comment claimed 16 bytes
(`projectile_form, flags, damage, value`). Census over `Skyrim.esm`: 35/35 records are 20 bytes;
the trailing `f32` at offset 16 is `0.1` in every one — a real, uniform per-arrow weight
`common.weight` was silently dropping.

## Verification

Independently re-derived the census against the mounted `Skyrim.esm` (throwaway
`crates/plugin/examples/_tmp_skyrim_ammo_data_census.rs`, deleted after use) — confirmed exactly:
**35/35 AMMO DATA sub-records, all 20 bytes, trailing f32 = 0.1 in all 35**, matching the audit's
own count precisely.

## Fix implemented

Corrected the comment (20 bytes) and read the trailing weight behind a `remaining() >= 4` check
(explicit, rather than relying solely on `f32_or_default`'s built-in truncation leniency) so a
genuine short record still decodes cleanly with weight left at `0`. `Fallout76`/`Starfield`
remain bundled in the same arm, unchanged — **explicitly not assumed 20-byte-Skyrim-shaped**;
they still need their own census before any length-dependent change, per the issue's own
deferral.

Regression tests (issue's own TESTS checklist item): the existing
`skyrim_ammo_data_is_projectile_form_flags_damage_value` (a deliberately truncated 16-byte
fixture) now also asserts `weight == 0.0` — proving a short record doesn't panic. New
`skyrim_ammo_data_includes_trailing_weight` pins the real, full 20-byte shape (a real Skyrim
form ID, `0x0001397D`, from the census) to `weight ≈ 0.1`.

**SIBLING** (issue's own checklist item): census-verified `parse_weap`'s Skyrim WEAP DATA arm
too (same throwaway-probe methodology) — **2,484/2,484 records confirmed exactly 10 bytes**,
matching its existing test's assumption; no bug found there. `parse_armo` has no per-game DATA
dispatch to check. This closes out the sibling gap #3716 (BOOK) flagged as belonging to this
issue.

Full workspace: `cargo test --no-fail-fast` 7049 passing, 0 failing.
