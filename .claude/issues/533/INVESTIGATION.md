# #533 Investigation — M33-01 NAM0 parse gate

## Domain
**esm** — record parser at `crates/plugin/src/esm/records/weather.rs:133`.

## Root cause

Single-format assumption. Parser expects `240 B = 10 groups × 6 TOD × 4 B`.
On-disk reality per game:

| Game | NAM0 size | TOD slot count |
|---|---|---|
| Oblivion | 160 B | 4 (SUNRISE/DAY/SUNSET/NIGHT) |
| FO3 | 160 B | 4 |
| FNV most | 240 B | 6 (+ HIGH_NOON, MIDNIGHT) |
| FNV ~12/63 | 160 B | 4 (older/DLC) |

Byte evidence (from audit's throwaway harness, now deleted):
```
OBL SEClearTrans NAM0 160 B: 6c 9d 9d 00 | 6c 9d 9d 00 | 6c 9d 9d 00 | 00 03 0d 00
                             ^sunrise      ^day          ^sunset       ^night
FO3 MegatonFalloutDe NAM0 160 B: 9d ac a4 00 | c8 db d1 00 | 70 70 65 00 | 16 25 1e 00
FNV NVWastelandClear* NAM0 240 B: 32 34 89 00 | 32 47 87 00 | 49 49 7a 00 | ...
```

## Fix design

Accept both on-disk sizes via the outer length guard (`>= 160`). Dispatch
inner stride on observed `sub.data.len()`:
- ≥ 240 → read 6 slots per group (current path)
- ≥ 160 → read 4 slots per group; synthesise `HIGH_NOON ← DAY`,
  `MIDNIGHT ← NIGHT` so the `[[SkyColor; 6]; 10]` struct stays the
  authoritative layout for downstream consumers (`systems.rs:1197-1226`,
  `build_tod_keys`).

## Files touched

1. `crates/plugin/src/esm/records/weather.rs` — NAM0 arm + new unit test
2. `crates/plugin/tests/parse_real_esm.rs` — add non-zero-colour assertion
   on FO3 default weather (opt-in `--ignored` test)

2 files, well under the 5-file scope threshold.

## Deferred

- **M33-07** (`#539`) — GameKind threading is a separate issue. This fix
  uses on-disk size as the dispatch key; once #539 lands, the dispatch
  can fold into a per-game schema.
- **`systems.rs:1197-1226` 4-slot mode** — with synthesis above, the
  interpolator sees a valid 6-slot table on every record, so no change
  needed. `build_tod_keys` still emits 7 keys with indices in [0, 6).
