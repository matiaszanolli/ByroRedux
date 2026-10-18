# SCR-D6-2026-10-17-01: `substitute_numbers` precision is taken unbounded from mod text - panic at >= 65 536, ~64 KB output per conversion below that

Issue: #6011
Labels: high, scripting, bug, safety

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D6-2026-10-17-01: `substitute_numbers` precision is taken unbounded from mod text - panic at >= 65 536, ~64 KB output per conversion below that

- **Severity**: HIGH
- **Dimension**: Legacy ObScript
- **Location**: `crates/scripting/src/messages.rs:189-232` (precision parse `:218-225`, `format!("{value:.precision$}")` at `:226`); reached from `obscript_quests.rs:215` -> `messages.rs:169`
- **Status**: NEW (predates `1502fbd3d`, which rewrote the parse but kept it unbounded; present since `98f707493`/`5182f7817`, 2026-10-13, after the baseline)
- **Untrusted-Input**: Yes
- **Description**: The `Message`/`MessageBox` text comes straight out of SCDA (`decode_message`, up to 65 535 bytes, no content check). `substitute_numbers` accumulates any run of `.`/digits after `%` into `spec`, then `precision = precision.parse::<usize>().unwrap_or(6)` with no upper bound, and feeds it to `format!("{value:.precision$}")`.
- **Evidence** (copy of the function compiled in a scratch dir with rustc 1.96.0, the repo's pinned toolchain):
  - `substitute_numbers("Rank %.65536f", &[3.0])` -> `panicked: Formatting argument out of range` (Rust's fmt precision is a u16; `%.65535f` is accepted and emits 65 537 bytes).
  - `"%.65535f".repeat(2000)` (16 KB of text) -> a 131 074 000-byte result; a 64 KB text (8 192 conversions) is ~537 MB for one message, then cloned into `ScriptMessage` / `MessageBoxQueue`.
  - `%.99999999999999999999f` fails `usize` parse -> falls back to 6 (harmless); `%.70000f` is the panic trigger. There is no `catch_unwind` around the Update-stage `obscript_quest_tick_system` (grep: none in core scheduler/byroredux), so the panic takes down the engine process as soon as the (start-game-enabled) quest ticks, i.e. a mod-triggered crash on load.
  - Width is ignored (`spec` is only used for `split_once('.')`), so a huge *width* allocates nothing; only the precision is the problem.
- **Impact**: Any plugin whose quest script carries `Message "%.70000f", x` crashes the game; `%.65535f` repeated exhausts memory (multi-hundred MB per statement per 5 s tick). The same function also formats MESG record text (`messages.rs:337`, Papyrus `Message.Show`), so a malformed ESM MESG hits it without any script.
- **Related**: #5789 (closed; the parse that this finding lives in), #5791 (VM alloc caps, same house rule).
- **Suggested Fix**: Clamp precision (e.g. `.min(20)`; f32 has no digits beyond ~17 significant) and cap the total output length of `substitute_numbers`; add a test with `%.65536f` and `%.99999f`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
