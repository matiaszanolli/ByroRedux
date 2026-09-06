# #3919: FO3-2026-09-05-D2-02: the two FO3 gates that catch D2-01 are `#[ignore]`d, and CI runs only `cargo test --workspace`

Filed from `docs/audits/AUDIT_FO3_2026-09-05.md` (FO3-2026-09-05-D2-02) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `medium,game:fo3,legacy-compat,nif-parser,test-gap,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3919 --json state`.

---

**Source**: `docs/audits/AUDIT_FO3_2026-09-05.md` (FO3-2026-09-05-D2-02), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: 2 / test infrastructure
- **Location**: `.github/workflows/ci.yml` (`cargo test --workspace`, no `--ignored`
  job); `crates/nif/tests/parse_real_nifs.rs` (`MIN_RECOVERABLE_RATE = 1.0`, no clean-rate
  floor); `crates/nif/tests/per_block_baselines.rs`; `crates/nif/tests/block_coverage_baselines.rs`.
- **Status**: NEW (adjacent to `#3850`, open — "at least 101 of 182 `#[ignore]`d
  real-data tests report a green `ok` when their data is absent"; that issue is about
  *silent skips*, this is about *never being invoked at all*).
- **Description**: The project **does** own guards that detect D2-01 precisely —
  `per_block_baseline_fallout_3` names the exact block type and the exact delta
  (`NiSkinPartition 0 -> 296`), and `unknown_ceiling_fallout_3` fails on the recovery
  count. Neither runs anywhere automatic. `ci.yml`'s test job is `cargo test --workspace`,
  which skips `#[ignore]`d tests; the only `--ignored` invocation in any workflow is
  `rt-correctness.yml`'s Cornell RT oracle. Meanwhile the gate that *is* effectively the
  advertised FO3 number — `parse_rate_fallout_3` — asserts only
  `recoverable_rate() >= 1.0`, and truncation counts as recoverable, so a 100 % → 98.29 %
  clean-rate collapse leaves it green.
- **Evidence**: `grep -rn "ignored" .github/workflows/*.yml` returns exactly one line,
  in `rt-correctness.yml`. `parse_real_nifs.rs` defines `MIN_RECOVERABLE_RATE` and no
  `MIN_CLEAN_RATE`; the two assertions at the tail of `run_game` are on
  `totals.total > 0` and `totals.recoverable_rate()`.
- **Impact**: A parse regression that drops 296 blocks of vanilla content across two
  shipped titles landed on `main` on 2026-09-03 and was still there on 2026-09-05.
  The detection latency is bounded only by how often someone runs a per-game audit by
  hand. This is a structural gap, not a one-off: the same shape hides any future
  clean-rate regression on any game.
- **Related**: `#3850`, `#3849` (the only pixel-level render guard cannot pass),
  `#3893` (Dim 9 discovery recipe blind spots).
- **Suggested Fix**: Two independent moves, either of which would have caught this.
  (a) Add a `MIN_CLEAN_RATE` per game to `parse_real_nifs.rs`, seeded from the
  checked-in per-block baselines, so the clean-rate is an assertion rather than a
  printed aside. (b) Add a CI job that runs the real-data corpus gates on a runner
  with game data, or — failing that — make the *absence* of data a hard failure in a
  nightly lane rather than a silent `ok` (the `#3850` half).

---

### Dimension 3 — ESM record coverage (`Fallout3.esm`)

**Verified clean.** `parse_rate_fo3_esm` passes at HEAD with every baseline exact:

```
[FO3] total=44718 | items=1762 containers=535 LVLI=972 LVLN=89 LVLC=60
      NPCs=1647 creatures=533 factions=326 globals=155 game_settings=530
      scripts=1257 trees=9
```

- `index.total()` = **44 718**, matching the #3756 re-measurement exactly (the
  index-sum metric, not a file record count; floor 44 000).
- The cell-tier floors `index.total()` cannot see both hold: placed refs ≥ 573 000
  and exterior cells ≥ 41 900.
- **#3542 / #3753 confirmed present.** The FO3 placed-mine chain that
  `AUDIT_FO3_2026-08-30.md` filed as `FO3-2026-08-30-D3-01` is fixed and gated: all
  four `PROJ` mine bases (`MineFragProjectile` `0x000043FA`, `MinePlasmaProjectile`
  `0x000403D8`, `MinePulseProjectile` `0x00033C4B`, `MineBottleCapProjectile`
  `0x00059449`) resolve in `cells.statics` with non-empty model paths.
- SCPT = **1 257**, unchanged. FO3's `SCHR` `u16` flags read (#1654) is untouched.
- WTHR/`NAM0` (#533) and `DNAM` (#534) guards pass.
- **REFR texture overlays**: not audited, per the #3511 correction — FO3 authors
  0 `XATO` / 0 `XTNM` / 0 `XTXR`.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
