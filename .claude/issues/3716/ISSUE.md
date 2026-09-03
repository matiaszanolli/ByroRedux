# #3716 — ESM-2026-08-30-D2-01: Skyrim BOOK.DATA is decoded with the 10-byte FNV layout against a 16-byte record — value and weight are read 2 bytes early and are garbage

**Severity**: MEDIUM · **Location**: `crates/plugin/src/esm/records/items.rs::parse_book`
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D2-01)

`parse_book`'s DATA arm grouped `Oblivion | Fallout3NV | Skyrim | Fallout76 | Starfield` under
the FNV-modeled 10-byte decode (`flags, skill, value(u32)@2, weight(f32)@6`). Skyrim's record is
16 bytes; every Skyrim book got a ~4-billion-scale `value` and a near-zero denormal `weight`.

## Verification

Independently re-derived the census against the mounted `Skyrim.esm` (throwaway
`crates/plugin/examples/_tmp_skyrim_book_data_census.rs`, using `EsmReader`'s public GRUP-walk
API directly since the crate's own visitor helpers are `pub(super)`; deleted after use) —
confirmed exactly: **821 BOOK DATA sub-records, all 16 bytes**. Sample:
`04 00 00 00 EC DD 10 00 DA 02 00 00 00 00 80 3F` → old reading (`value`@2, `weight`@6) =
(3,723,231,232, 3.2e-34); offset-8/12 reading = value 730, weight 1.0 — self-evidently correct
(book price/weight), with bytes 4..8 (`0x0010DDEC`) resolving as a real FormID (the skill-book
"Teaches" reference), matching a plausible in-game skill-book AVIF.

## Fix implemented

`GameKind::Skyrim` gets its own match arm (16 bytes): `flags(u8), skill_type(u8), unknown(u16),
teaches(u32 AVIF FormID), value(u32), weight(f32)` — `teaches` routes into the existing
`teaches_skill` field, previously fed only by `SKIL` (which Skyrim does not emit). `skill_bonus`
stays `0` for Skyrim (the 16-byte layout has no equivalent byte).

Implemented as a **compile-time-enforced split** rather than the suggested runtime length guard:
`GameKind::Skyrim` no longer appears in the shared 10-byte arm's pattern at all, so a 16-byte
Skyrim record can never reach the 10-byte decode by construction — stronger than a fallible
runtime check on a still-shared arm. `Oblivion | Fallout3NV | Fallout76 | Starfield` remain on
the 10-byte decode unchanged; **FO76/Starfield explicitly still need their own census** before
being moved, per the issue's own deferral — not assumed 16-byte-Skyrim-shaped.

Regression tests (issue's own TESTS checklist item): `skyrim_book_data_is_16_bytes_teaches_value_weight`
pins the real 16-byte payload above to `value=730`/`weight=1.0`/`teaches_skill=0x0010DDEC`/`flags=4`;
`skyrim_book_data_none_teaches_sentinel_is_preserved` pins a second real sample (a book teaching
no skill, `teaches=0xFFFFFFFF`, the Bethesda "none" sentinel — not `0`).

**SIBLING** (issue's own checklist item): checked `parse_weap`/`parse_armo`/`parse_ammo` for the
same over-broad `Skyrim | Fallout76 | Starfield` grouping. `parse_weap`'s WEAP DATA arm already
has its own dedicated Skyrim-family test (`skyrim_weap_data_is_10_bytes_no_health_no_clip`) —
not the same unverified-grouping shape. `parse_armo` has no per-game DATA dispatch. `parse_ammo`'s
AMMO DATA arm groups the same three games under a 16-byte layout whose own comment is **already
independently flagged as wrong by a separate filed issue, #3723** ("Skyrim AMMO.DATA is 20 bytes
on disk, not the 16 the decoder's comment claims") — not fixed here, left to that issue.

Full workspace: `cargo test --no-fail-fast` 7048 passing, 0 failing.
