# #7102 — ESM-D1-04: `weekday_origin` reads a defaulted HEDR as established FO3; its Skyrim "no HEDR" arm is unreachable

Labels: info,esm-plugin,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D1-04)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D1-04**

- **Severity**: INFO (malformed input only; no installed plugin triggers it)
- **Dimension**: 1, missing/short-HEDR fallback
- **Record / Sub-record**: TES4 / HEDR, `GLOB` `GameYear` / `GameMonth` / `GameDay`
- **Location**: `crates/plugin/src/esm/records/calendar.rs:52-67`; call at `records/parse.rs:566-567`; first-wins merge in `records/index.rs` (`if self.weekday_origin.is_none()`)
- **Status**: NEW (the same class as the fixed #6780, in a new consumer from `02909f1ee`, #6658)
- **Description**: `read_file_header` leaves `hedr_version = 0.0` when HEDR is absent or under 12 bytes, and `from_header` then falls to `Fallout3NV`. `weekday_origin(Fallout3NV, 0.0, ..)` returns `Some(gregorian().unwrap_or(0))`, a Sunday origin presented as established. The Skyrim arm and its test (`weekday_origin(Skyrim, 0.0, None) == None`, "no HEDR read: not established") can never be reached from `parse_esm`, because a defaulted header never yields `GameKind::Skyrim`. Since the merge keeps the first established origin, a headerless first plugin would pin Sunday for the whole load order.
- **Evidence**: The `from_header` fallthrough is verified at `reader.rs:313-328`; all 120+ installed headers carry a real HEDR (census below).
- **Impact**: A garbled or headerless first plugin gets a made-up weekday origin instead of `None`. The two lineages share the rest of the schema.
- **Suggested fix**: Optional. Pass `hedr_version > 0.0` as "HEDR present" into `weekday_origin`, so a defaulted header yields `None` like the Skyrim arm intends. Or drop the unreachable arm and test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
