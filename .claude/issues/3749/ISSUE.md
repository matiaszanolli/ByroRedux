# #3749 — TD9-2026-08-30-01: 80% of `#[ignore]`s carry no machine-readable reason

**Severity**: TECH-DEBT · **Location**: workspace-wide (`.rs` test files)
**Source**: `docs/audits/AUDIT_...` sweep 2026-08-30

## Premise, re-verified against current HEAD

The issue's own counts were stale by the time this fix landed (unsurprising —
the workspace gained/lost a few `#[ignore]` sites across the sessions between
the audit and this fix). Re-measured against current HEAD before touching
anything:

- **138** bare `#[ignore]` sites (no reason string) across **27** files —
  not the issue's cited 136.
- **18** distinct existing `#[ignore = "..."]` reason strings already in the
  tree — not the issue's cited 14 — predominantly of the shape
  `"needs <GAME> game data on disk"` and
  `"needs a working audio device and <GAME> game data"`.

The premise itself held: the overwhelming majority of gated tests genuinely
carried no reason, forcing a reader (or, per the issue, an audit tool
building an `--ignored` baseline) to open every function body to learn why a
test is skipped. That's exactly the failure mode that produced two wrong
audit baselines (#3440, #3456).

## Fix

Systematic, function-scoped conversion of every bare `#[ignore]` to
`#[ignore = "<reason>"]`, one file at a time, verifying the true skip
condition against each function's actual body (never a fixed-line-window
guess — see METHODOLOGY below) rather than trusting inferred defaults.

138 sites converted across 27 files:

- `crates/plugin/tests/parse_real_esm.rs` (24)
- `crates/nif/tests/parse_real_nifs.rs` (13)
- `crates/plugin/src/esm/cell/tests/integration.rs` (12)
- `crates/plugin/src/esm/records/tests.rs` (11)
- `crates/bsa/src/archive/tests.rs` (11)
- `crates/nif/tests/per_block_baselines.rs` (7)
- `crates/nif/tests/block_coverage_baselines.rs` (7)
- `crates/bsa/tests/ba2_real.rs` (7)
- `crates/audio/src/tests.rs` (6)
- `byroredux/src/npc_spawn/tests.rs` (4)
- `byroredux/src/cell_loader/precombined.rs` (4)
- `crates/nif/tests/ragdoll_import.rs` (4)
- `crates/bsa/tests/bsa_real.rs` (4)
- `crates/plugin/src/esm/records/actor/tests.rs` (3)
- `crates/facegen/tests/parse_real_facegen.rs` (3)
- `crates/spt/tests/parse_real_spt.rs` (3)
- `crates/bgsm/tests/parse_all.rs` (2)
- `byroredux/tests/m41_phase1bx_skinning.rs` (2)
- `byroredux/src/render/draw_sort_key_tests.rs` (2)
- `crates/plugin/src/equip.rs` (1)
- `crates/sfmaterial/tests/real_cdb.rs` (1)
- `crates/nif/tests/translation_completeness.rs` (1)
- `crates/nif/tests/mtidle_motion_diagnostic.rs` (1)
- `crates/nif/tests/common/mod.rs` (1 — a doc-comment usage example, not a
  live test attribute; updated anyway so copy-pasting it doesn't propagate
  the bare form)
- `byroredux/src/systems/animation.rs` (1)
- `byroredux/src/npc_spawn/ai_package.rs` (1)
- `byroredux/src/cell_loader/load_order.rs` (1)
- `crates/bsa/tests/csg_real.rs` (1)

Reason strings mostly follow the established `"needs <GAME> game data on
disk"` shape. Two sites got a different category entirely on inspection —
`byroredux/src/render/draw_sort_key_tests.rs`'s two `manual_bench_*`
functions are one-shot timing calibration gates, not game-data-gated tests;
their module doc already said so ("the timings are environment-dependent"),
so they got `"manual timing bench, environment-dependent"` instead of a
game-data reason.

## METHODOLOGY

A fixed-line-window heuristic (peek N lines after the `#[ignore]` line) was
tried first and produced two classes of wrong answers before being
abandoned in favor of proper per-function scoping (locate the enclosing
`fn`, brace-match to its closing `}`, search only within that span):

1. **Window bleed across short functions** — a 40-line window overran into
   the *next* function's env-var references, mislabeling
   `dlccoast_header_classifies_as_fallout4` (FO4-only) as needing "FNV/FO4".
2. **Digit-excluding regex** — `[A-Z_]+?` doesn't match `FO4`/`FO76` (they
   contain digits), silently producing an empty hint for every FO4/FO76 site
   until widened to `[A-Z0-9_]+?`.

Every site converted this pass was verified by reading its actual function
body (or, for the four files not caught by the automated pass —
`equip.rs`, `npc_spawn/tests.rs`, `cell_loader/precombined.rs`,
`ragdoll_import.rs`, `esm/records/actor/tests.rs`,
`m41_phase1bx_skinning.rs`, `draw_sort_key_tests.rs`, `real_cdb.rs`,
`translation_completeness.rs`, `mtidle_motion_diagnostic.rs`,
`common/mod.rs`, `systems/animation.rs`, `npc_spawn/ai_package.rs`,
`cell_loader/load_order.rs`, `bsa_real.rs`, `csg_real.rs` — by direct
inspection), not inferred from a hint table.

`translation_completeness.rs`'s `cross_game_translation_completeness` is the
one genuinely multi-game site: it iterates `HARNESS_GAMES` (all 7 supported
games, self-skipping per-game), so its reason names the full set rather than
a single game.

## TESTS (the issue's own checklist deliverable)

Per the issue's framing — *"the fix *is* the test... a CI check that every
`#[ignore]` carries a reason string makes the convention self-enforcing"* —
added `every_ignore_attribute_carries_a_reason` to
`byroredux/src/workspace_hygiene_tests.rs`, alongside the structurally
identical `#3746` `_tmp_*`-example guard already living there. It walks the
whole workspace tree from `CARGO_MANIFEST_DIR/..` (skipping `target/` and
`.git/`), and flags any line whose trimmed content is exactly `#[ignore]`
(doc-comment mentions of the token don't match, since they're never the
*entire* trimmed line).

Verified the guard actually catches the regression it exists to prevent:
added a throwaway bare-`#[ignore]` test file + `mod` declaration, confirmed
`every_ignore_attribute_carries_a_reason` failed with the expected message
naming the exact file:line, then reverted (`git checkout -- main.rs` + `rm`)
and confirmed it passes clean again.

## Verification

- `cargo check -p <every touched crate> --tests`: clean (one pre-existing,
  unrelated `unused_mut` warning in `esm/records/grup_walker.rs` predates
  this session's changes to that file — not introduced by this fix, out of
  scope).
- `cargo test -q -p <every touched crate>`: all passing, 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7074 passing, 0
  failing** (+1 new guard test).
- `grep -rn '^\s*#\[ignore\]\s*$' --include='*.rs' .` (excluding `target/`):
  zero matches — confirms the conversion is complete workspace-wide.
