# #3724 — ESM-2026-08-30-D2-03: the item DATA arms dispatch on GameKind alone with no length validation

**Severity**: LOW · **Location**: `crates/plugin/src/esm/records/items.rs` — `parse_weap` (`DATA`, `CRDT`), `parse_armo` (`DATA`, `DNAM`), `parse_ammo` (`DATA`), `parse_keym` (`DATA`), `parse_book` (`DATA`)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D2-03)

These were the only fixed-layout multi-field arms in the crate with no length
guard at all; every other decoder gates on `sub.data.len()`. `*_or_default`
leniency is safe against truncation-to-zero (a failed read doesn't advance
the cursor — see `SubReader`'s doc comment) but not against a partial
truncation immediately followed by a *narrower* field: the failed wide read
leaves the cursor exactly where it was, and the following narrow read then
succeeds by consuming bytes that belonged to the wide field's range —
producing a garbage value for the narrow field instead of also
zero-defaulting it.

Worked case (the issue's own): a 13-byte FO3/FNV `WEAP DATA` (real width 15
bytes) truncates inside `damage(u16)`; the failed `u16_or_default` leaves the
cursor unmoved, so the following `clip_size = u8_or_default()` silently
consumes `damage`'s one stray remaining byte.

## Fix implemented

Added a `sub.data.len() >= N` guard to every per-`GameKind` branch across all
seven listed arms, using each branch's own measured on-disk width already
documented in its comment:

- `parse_weap` `DATA`: Oblivion (30 B), FO3/FNV (15 B), Skyrim/76/Starfield
  (10 B); FO4 has no `DATA` (unchanged, empty arm).
- `parse_weap` `CRDT`: 8 B (shared).
- `parse_armo` `DATA`: Oblivion (14 B), FO3/FNV (12 B), FO4 (12 B),
  Skyrim/76/Starfield (8 B).
- `parse_armo` `DNAM`: FO3/FNV (8 B), Skyrim/76/Starfield (4 B).
- `parse_ammo` `DATA`: Oblivion (18 B), FO3/FNV (13 B), FO4 (8 B),
  Skyrim/76/Starfield (16 B leading fields — the trailing weight field keeps
  its own separate `remaining() >= 4` check from #3723 rather than folding
  it into the 20-byte outer guard, so a genuine 16-byte FO76/Starfield
  record — not yet census-verified as 20-byte-Skyrim-shaped — still decodes
  its first four fields cleanly).
- `parse_keym` `DATA`: 8 B (shared, game-invariant).
- `parse_book` `DATA`: FO4 (8 B), Skyrim (16 B, #3716), Oblivion/FO3NV/76/SF
  shared arm (10 B).

A branch whose guard fails now falls through to an explicit `{}` arm rather
than partially decoding — every field it would have set stays at its
already-initialized zero default. `match game { ... }` needed an explicit
no-op arm per variant once guards were added, since a guard doesn't count
toward exhaustiveness on its own.

**SIBLING** (issue's own checklist item): all seven listed arms guarded in
this one pass — not just `WEAP`.

**TESTS** (issue's own checklist item):
`truncated_fo3nv_weap_data_does_not_field_shift` reproduces the issue's exact
worked case — a 13-byte FO3/FNV `WEAP DATA` ending mid-`damage` — and asserts
every field (`value`, `weight`, `damage`, `clip_size`) lands at its zero
default rather than `clip_size` picking up `damage`'s stray byte.

Full workspace: `cargo test --no-fail-fast` 7055 passing, 0 failing (+1 new
test).
