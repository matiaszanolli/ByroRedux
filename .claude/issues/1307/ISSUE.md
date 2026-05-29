# #1307 -- OBL-D3-03: DIAL dialogue-type byte unparsed

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: LOW | **Dim 3** — ESM Record Coverage
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D3-2026-05-28-03)

**Location**: `crates/plugin/src/esm/records/misc/ai.rs:269-284` (DialRecord), `:314-332` (parse_dial)

**Issue**: DIAL `DATA` dialogue-type byte (Topic/Greeting/Combat/Service/Persuasion/…) is parsed for none of Oblivion's 3817 DIAL records. All DIAL records collapse to an undifferentiated `DialRecord`. No current render impact; latent gap for future dialogue/quest runtime work.

**Suggested fix**: add `pub dial_type: u8` to `DialRecord` and a `b"DATA" if !sub.data.is_empty() => out.dial_type = sub.data[0]` arm in `parse_dial`. (FO3+ DATA is 4 bytes but byte 0 is the type in all games, so the single-byte read is cross-game safe.)

## Completeness Checks
- [ ] **SIBLING**: verify FO3/FNV DIAL DATA parsing captures the same byte
- [ ] **TESTS**: unit test asserting dial_type is populated for a known Oblivion DIAL record
- [ ] **CANONICAL-BOUNDARY**: ESM parse-side only
- [ ] **UNSAFE**: no unsafe involved
