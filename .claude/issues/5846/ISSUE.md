# TD7-2026-10-15-04: `PROJ` `DATA` is decoded by absolute offsets paired with hand-typed length gates

Labels: low,esm-plugin,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD7-2026-10-15-04`).

- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants (ESM sub-record sizes)
- **Location**: `crates/plugin/src/esm/records/misc/effects.rs:182-211` (`len() >= 12`, `>= 40` + `[36..]`, `>= 56` + `[52..]`, `>= 80` + `[72..]` / `[76..]`)
- **Status**: NEW
- **Age**: `9e6c44cea` (2026-10-14, "Weapon.Fire launches a projectile")
- **Effort**: trivial
- **Description**: every other record in `crates/plugin/src/esm/records/` walks a `SubReader` sequentially; this one is the only production site (4 hits repo-wide) that re-slices `SubReader::new(&sub.data[N..])` and guards each with `len() >= N+4`. The offset and the gate must be edited together; the comment at `:188-189` documents "16..36 are lights, tracer chance..." instead of skipping them with the reader's own `skip_or_eof(16)`.
- **Evidence**: `if sub.data.len() >= 40 { let mut r = SubReader::new(&sub.data[36..]); ... }`.
- **Impact**: a game-variant tail change (the code already splits Skyrim+ vs FO3/FNV at offset 72) needs two coordinated literal edits; a mismatch silently returns a default instead of an error.
- **Related**: `crates/plugin/src/esm/sub_reader.rs:111` (`skip_or_eof`).
- **Suggested Fix**: one `SubReader` pass with `skip_or_eof` between the named fields (flags, gravity, speed, range, [skip lights/tracer/proximity/timer], explosion, [skip], impact force, [skip], collision radius, lifetime), each `*_or_default()` covering a short tail.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
