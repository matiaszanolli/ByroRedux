# #1313 (OBL-D3-03) — ALREADY FIXED (duplicate of closed #1307)

No code change. The DIAL `DATA` dialogue-type byte is fully parsed; #1313
(`OBL-D3-2026-05-28-03`) is a re-file of the finding already resolved by #1307.

Current `crates/plugin/src/esm/records/misc/ai.rs` (premise now stale):
- `:283 pub dial_type: u8` on `DialRecord`.
- `:346 b"DATA" if !sub.data.is_empty() => out.dial_type = sub.data[0]` in `parse_dial`
  (comment at :345 cites `#1307 / OBL-D3-...-03`; notes FO3+ DATA is wider but byte 0 is
  still the type — cross-game safe).
- Tests: `:633` (DATA absent → 0/Topic), `:648` (Oblivion type 3), `:652` (FO3 subs →
  type 5 — the SIBLING check), `:656` (empty → 0).

All of #1313's suggested-fix bullets + completeness checks (field, DATA arm, test,
FO3/FNV SIBLING) are present. Closed as duplicate of #1307.
