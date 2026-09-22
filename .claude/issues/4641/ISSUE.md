# ESM-2026-09-21-D2-02: #4171's TERM fix still drops most Fallout 4 terminal text — UNAM display text, WNAM/NAM0, and conditional BTXT bodies

**Issue**: #4641
**Filed**: 2026-09-22 (audit-publish, AUDIT_ESM_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: Sub-Record Byte Accounting (with Localized-Strings consequences)
**Game Affected**: Fallout 4
**Location**: `crates/plugin/src/esm/records/misc/world.rs:1544-1619` (`parse_term`), `:1517-1542` (`TermRecord`)

## Description
FO4 `TERM` layout: `NAM0`/`WNAM` lstrings, a `BSIZ`-counted array of `{BTXT, Conditions}` bodies, and menu items `{ITXT, RNAM, ANAM Type, ITID, UNAM 'Display Text', VNAM, TNAM, Conditions}`. For `ANAM = 8` items, the shown text is `UNAM`. `parse_term` has no `NAM0`/`WNAM`/`UNAM` arm; keeps a single `body_text` (last `BTXT` wins, conditions discarded).

## Evidence
- xEdit `wbDefinitionsFO4.pas` `wbRecord(TERM …)`.
- `Fallout4.esm` census (778 TERM records): 1,818 menu items have `ANAM=8`, 1,804 carry text in `UNAM`; 467 author `WNAM`, 291 author `NAM0`; 82 author 2-7 conditional `BTXT` bodies.

## Impact
Latent today (nothing consumes `TermRecord` text yet), but the parser is the only place this content can be recovered — currently it keeps entry titles and drops nearly every FO4 terminal log body. A future terminal UI built on this decode would render FO4 entries empty.

## Suggested Fix
Add `UNAM` per-item `display_text` (via `read_lstring_or_zstring`). Add record-level `NAM0`/`WNAM`. Model the body as `Vec<(text, conditions)>`.

## Related
#4171 (closed — named only ITXT/BTXT/RNAM; this is the residual)

## Source
docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D2-02)
