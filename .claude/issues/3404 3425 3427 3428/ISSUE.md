# #3404, #3425, #3427, #3428

## #3404 — ESM-2026-08-27-D2-01: rows() silently drops a non-zero stride remainder
Labels: bug, low, esm-plugin
Location: `crates/plugin/src/esm/records/misc/world.rs`

`rows()` (used by `NAVM`'s `NVVX`/`NVTR`/`NVEX`/`NVDP`/`NVCA`) truncated a
non-zero stride remainder silently via `chunks_exact`, unlike every other
decoder in the file (`decode_nvgd`, `decode_nvnm`, `decode_weather_rows`),
which all refuse to return anything unless the payload reconciles exactly.

**Fix**: added `rows_exact()`, a strict sibling of `rows()` that returns
`None` on a non-zero remainder. `parse_navm` now reads `DATA`'s words 1-5
(vertex/triangle/external/cover/door counts — already established in the
corpus to reconcile 11,969/11,969) once up front and cross-checks each
typed sub-record's decoded row count against it; a stride-remainder miss
OR a count mismatch now degrades the field to empty, matching
`decode_nvgd`/`decode_nvnm`'s all-or-nothing posture instead of silently
half-filling it.

## #3425 — TD4-2026-08-27-01: the fix→issue link is only checked in one direction
Labels: bug, medium, tech-debt
Location: `scripts/check-issue-traceability.sh`

The 5 orphaned issues cited in the report (#3149, #3151, #3155, #3244,
#3270) were **already closed** by the time this session started (verified
via `gh issue view`) — that part of the suggested fix was already done.
The actual script gap — no mode to *detect* a fixed-but-never-closed
issue — was still present.

**Fix**: added a third `--orphan <base> <head>` mode. It scans the `.rs`
diff of the range for `#NNNN` references added on a `+` line, and for each
one that's still `OPEN` with no closing-keyword commit in range, lists it
as a candidate orphan (advisory, like `--window` — a forward-looking `#N`
reference is a legitimate pattern it can't distinguish from a forgotten
keyword). Wired into `session-close`'s ritual alongside `--window`.
Manually verified against this repo's own real history (`HEAD~1..HEAD`
correctly flagged #3809/#3810, both real OPEN research-spike issues
referenced in comments this session's prior commit added).

## #3427 — UI-D3-2026-08-27-02: ScaleformHostObjectState has no engine consumer
Labels: bug, medium, ui, game:fo4
Location: `crates/ui/src/avm2_host.rs`, `player.rs`, `byroredux/src/scene.rs`

`SwfPlayer::host_object_state()` existed but had no caller outside
`crates/ui`, and `UiManager` didn't re-export it — an AVM2 menu that
reached `NotPresent` logged identically to one that injected cleanly.

**Fix**: added `UiManager::host_object_state()`, folded into both menu-load
log lines in `scene.rs` (`ui.menu: loaded ... state={:?}` for the `--menu`
archive route, `UI texture registered ... state={:?}` for the `--swf`
loose-file route) — the `--menu` line's existing prefix/keys stay stable
for `m48-menu-load.sh`'s fixed-string grep.

## #3428 — UI-D3-2026-08-27-03: a lifecycle-class scan miss is a hard menu-load failure
Labels: bug, medium, ui, game:fo4
Location: `crates/ui/src/avm2_host.rs`

Two branches in `inject_into_parsed_movie` were a hard `Err` (aborting the
whole menu load) for cases that are really "this movie has no host
object": an unparseable ABC candidate tag propagated via `?`, and no
instance-level trait match falling through to `.ok_or_else(...)?`. The
sibling scan over the same tags 25 lines later (`referenced_host_methods_in_tags`)
was already deliberately non-fatal.

**Fix**: an unparseable ABC candidate is now skipped (logged, `continue`)
rather than aborting the scan; falling through with no resolved lifecycle
class now degrades to `Ok((None, ScaleformHostObjectState::NotPresent))`
with a `log::warn!`, matching the sibling's policy. The hard `Err` is kept
only for `patch_root_constructor`, where a partial rewrite really would
hand Ruffle a corrupt SWF. `marker_without_a_lifecycle_class_is_rejected`
renamed to `marker_without_a_lifecycle_class_degrades_to_not_present` and
updated to assert the degraded state instead of `.unwrap_err()`.

All four fixes verified: `cargo test -q -p byroredux-plugin -p byroredux-ui
-p byroredux` clean, zero new warnings; full `cargo test -q` workspace gate
clean (0 failures across every test binary).
