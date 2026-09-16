# #4417 — RT-2026-09-16-01: `BYROREDUX_FIXED_DT=0`, which the skill recommends, silently switches the bench to `renderer-static`, and every baseline was captured in `system-live`, so the draw-split rows can't be compared

**Labels**: medium,tech-debt,bug

**Source**: `docs/audits/AUDIT_RUNTIME_2026-09-16.md` (RT-1)

- **Severity**: MEDIUM
- **Status**: NEW
- **Dimension**: audit infrastructure (`.claude/commands/audit-runtime/SKILL.md` Notes, `capture.sh`, baseline TSVs)
- **Description**: The skill's *Notes → Determinism* paragraph recommends
  `BYROREDUX_FIXED_DT=0` "when capturing tolerance metrics".
  `resolve_bench_selection` (`byroredux/src/bench.rs` ~L194) maps that env var
  to `BenchMode::RendererStatic`. `capture.sh` never passes `--bench-mode`, so
  without the env var the engine runs `system-live`, and every committed
  baseline was captured that way. The TSVs don't record the mode, so a capture
  taken per the skill's own advice is diffed against a baseline from a
  different mode, and nothing flags it.
- **Evidence**: Same build and same cells, captured minutes apart. The
  `bench:` draw split in each mode:

  | Game | `renderer-static` (`FIXED_DT=0`) | `system-live` (unset) | Baseline |
  |------|------------------------------|-----------------------|----------|
  | oblivion | 330/**78b/5c**, raster **132** | 330/20b/2c, raster 22 | 330/20b/2c, raster 22 |
  | fnv | 2204/**167b/36c**, raster 283 | 2197/109b/26c, raster 188 | 2110/109b/26c |
  | fo3 | 1579/**114b**/12c, raster 123 | 1581/100b/11c, raster 108 | 1581/100b/11c |
  | skyrim_se | 2458/**13b/3c**, raster 14 | 2457/9b/2c, raster 9 | 2342/9b/2c |
  | fo4 | 3969/**196b/13c**, raster **256** | 3964/248b/16c, raster 359 | 3964/248b/16c |

  Read against the baselines, the static pass would have filed four phantom
  draw regressions: batches +53 % on fnv, +290 % on oblivion, +44 % on skyrim
  and +14 % on fo3. In the static pass's own logs, the once-per-second
  `engine::stats` lines printed *after* the `bench:` line read 109b / 20b /
  100b / 9b again. `--bench-hold` puts the engine back into live
  wall-clock mode, so the split is tied to the frozen-dt window, not to the
  scene. Separately, the static-mode FO4 run exited before any
  `engine::stats` boundary, so its three `skin_pool_*` rows were not captured.
- **Contract conflict**: `BenchMode` in `bench.rs` documents `RendererStatic`
  as the mode for "deterministic regression gates only". It documents
  `SystemLive` as "combined system observation only; **never a regression
  gate**". The skill gates on exactly that mode today.
- **Open question (not a finding yet)**: why does a frozen `dt` push Oblivion
  from 22 to 132 raster commands and from 20 to 78 batches, with the same 330
  total? One candidate: a dt-driven state (fade, alpha or visibility ramp)
  never advancing, which leaves more commands in the blended raster prefix.
  That is unverified. It matters because `renderer-static` is the mode the code
  says gates should use.
- **Suggested Fix**: Pick one mode and pin it. Pass `--bench-mode` explicitly
  from `capture.sh`, record it in each TSV (e.g. a `bench_mode` row the diff
  checks exact-match first), and delete or rewrite the `FIXED_DT` advice in
  *Notes*. If the pinned mode is `renderer-static`, as `bench.rs` intends,
  re-capture all five baselines in one run each, and answer the open question
  first so the baselines don't bake in an artifact of the frozen dt.

## Completeness Checks
- [ ] **SIBLING**: `capture.sh`, SKILL.md *Notes*, and every baseline TSV agree on one bench mode
- [ ] **TESTS**: `capture.sh --self-test` covers the pinned `--bench-mode` / TSV mode-row check
