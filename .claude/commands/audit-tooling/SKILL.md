---
description: "Audit of tooling and host-facing contracts — SDK (Studio snapshots, typed commands, StorageUtil compat), debug server + protocol + byro-dbg, debug-ui panels, launcher/boot-request/settings-io handoff, install detection, byro-detect, texture-upscale"
argument-hint: "--focus <dimensions> [--area sdk|debug|launcher|detect|tools]"
---

# Tooling and Host-Contract Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

Surfaces a modder, player or developer touches instead of the engine: a TCP command port evaluated against the live `World`, the renderer-independent SDK, the launcher's boot/settings handoff and the tool CLIs. Not here: `crates/mod-runtime` (`/audit-safety` Dim 8); debug-server locks/threads (`/audit-concurrency` Dim 7); the egui *pass* (`/audit-renderer`); VDF/ACF parsing (`/audit-parsers`); save schema (`/audit-save`).

| Area | Code (docs) |
|---|---|
| sdk | `crates/sdk/src/` (identity, manifest, `service.rs`, `studio.rs`, `compatibility/`); adapters `byroredux/src/studio_host.rs`, `byroredux/src/commands/studio.rs`, `byroredux/src/extensions/` (`docs/engine/sdk-v0.1-development-plan.md`, `docs/engine/sdk-v0.1-next-action-plan.md`) |
| debug | `crates/debug-server/src/{listener,system,evaluator,registration}.rs`, `crates/debug-protocol/src/{lib,wire,registry}.rs`, `tools/byro-dbg/src/`, `crates/debug-ui/src/` (panels also host the player menu, inventory, HUD) (`docs/engine/debug-cli.md`) |
| launcher | `crates/boot-request/src/`, `crates/settings-io/src/`, `crates/core/src/atomic_file.rs`, `tools/byro-launcher/src/`, `byroredux/src/boot/cli.rs` (`expand_boot_request`) (`docs/engine/launcher.md`) |
| detect | `crates/game-detect/src/{catalog,steam,profiles,validate,overrides}.rs`, `crates/core/src/ecs/game_profiles.rs`, `assets/debug_profiles.toml`, `tools/byro-detect/src/main.rs` (`docs/engine/launcher.md` §3) |
| tools | `tools/texture-upscale/src/` (`tools/nifskope` is vendored, not first-party) |

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: dimensions to run (default all 6). `--area <name>`: one area.
- Delta-first: run each `First step` and `git log --since=<last AUDIT_TOOLING date> -- <Paths>`; deep-audit only dimensions whose Paths changed.

## Extra Per-Finding Fields

- **Dimension**: Debug Trust Boundary | Protocol & Registry | SDK Contract | Boot Handoff & Persistence | Install Detection | Tool CLIs
- **Exposure**: who can reach it (any local process / launcher user / modder / developer only). Unauthenticated local file write or command execution in a stock build = HIGH; developer-only tool defect = LOW–MEDIUM; user data loss in a crash window = MEDIUM.

## Phase 1: Setup

1. `mkdir -p /tmp/audit/tooling`; `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/tooling/issues.json`; dedup per `_audit-common.md`.
2. Read the newest `docs/audits/AUDIT_TOOLING_*.md` if any.
3. `cargo test` for `-p byroredux-{sdk,debug-server,debug-protocol,debug-ui,boot-request,settings-io,game-detect}` and `-p byro-{launcher,detect,texture-upscale}`; record counts. Do not launch a windowed engine to exercise the debug server (*feedback_no_parallel_engine_launch*).

## Phase 2: Dimensions (max 3 concurrent agents)

### Dimension 1: Debug-server trust boundary
Paths: `crates/debug-server/src/{listener,system,evaluator}.rs`, `crates/debug-protocol/src/wire.rs`, `byroredux/src/main.rs` (start site), `byroredux/Cargo.toml`.
First step: `grep -n 'bind\|MAX_\|fs::write\|create_new\|from_secs' crates/debug-server/src/*.rs crates/debug-protocol/src/wire.rs`.
Facts (2026-09-29): bind is the literal `("127.0.0.1", port)`; no authentication; `debug-server` is a `default` feature of `byroredux`, started from `main.rs` on `BYRO_DEBUG_PORT` (default 9876), so every stock build exposes a command port to every local process. Caps: `MAX_QUEUED_COMMANDS` 64, `MAX_CONCURRENT_CLIENTS` 8 (#3449), 300 s read / 30 s response (`COMMAND_RESPONSE_TIMEOUT`) timeouts, 16 MB frame (`MAX_MESSAGE_SIZE`, enforced by both `encode` and `decode`), `MAX_WALK_DEPTH` 64.
Guards: `try_enqueue_rejects_when_queue_is_full`, `connections_past_the_cap_are_refused`, `occupied_port_is_reported_before_a_handle_is_returned` (`listener.rs`); `rejects_oversized_message`, `encode_rejects_response_payloads_over_the_protocol_limit` (`wire.rs`); `cancelled_screenshot_does_not_skip_same_frame_command_drain`, `rejects_absolute_and_parent_paths` (`system.rs`); `walk_depth_is_capped_even_for_untrusted_requests` (`evaluator.rs`). No test pins the loopback address: a `0.0.0.0` bind would pass every guard.
Checklist:
- Capability surface = the whole console: `eval_request` dispatches every registered dotted command; `SetField` writes any registered component field; `LoadNif { path }` reads absolute paths. File writes are confined (2026-09-27): `write_screenshot` (`system.rs`) takes a single filename under `screenshots/` with `create_new` (no overwrite), and `tex.dump` goes through `safe_tex_dump_path` (`byroredux/src/commands/assets.rs`, `texture-dumps/`) — which has no test; check both still reject absolute, `..` and nested paths and that no other command writes a client-chosen path. Ask whether each mutating or file-writing command (`tex.dump`, screenshot, dumps) should be reachable unauthenticated in a *player* build; size by Exposure.
- Evaluator bounds: `parse_expr` runs on the engine thread in the Late exclusive (depth caps in `crates/papyrus/src/parser/`); `WalkEntity.max_depth` is clamped by `bounded_walk_depth`; other entity/`Value` responses are unbounded until `encode` refuses them.
- An over-limit response is replaced by an error response in `handle_client` (the `InvalidData` arm) rather than a torn frame — verify the fallback itself cannot exceed the limit and that the command's side effects already ran. A decode error (even an unknown variant) drops the connection: intended?
- The drain system runs `evaluate` inline, so a slow query stalls a frame; is there a per-request cost bound? Lock *order* is `/audit-concurrency` (`debug_evaluator_acquires_locks_in_canonical_order`). Abandoned requests (`PendingCommand.cancel`, #1007) and the `SCREENSHOT_OWNER_*` claim must release on client timeout.

### Dimension 2: Protocol and registry completeness, wire compat
Paths: `crates/debug-protocol/src/{lib,registry}.rs`, `crates/debug-server/src/registration.rs`, `tools/byro-dbg/src/{main,display,tui}.rs`, `docs/engine/debug-cli.md`.
First step: `grep -c 'register_component::<' crates/debug-server/src/registration.rs`; diff against inspect-derived components (`grep -rn 'cfg_attr(feature = "inspect"' crates/core/src`).
- Re-derive counts: `docs/engine/debug-cli.md` "Currently registered (N components)" must equal the live `register_component::<` count (both 49 on 2026-09-29).
- The only completeness guard is `roster_tests::every_ai_procedure_behavior_component_is_registered` (`*Behavior` types, #4063). For the rest, enumerate inspect-derived components minus registered ones and judge which hold user-visible state `byro-dbg` cannot inspect (recent gameplay components are the likely gap).
- Wire: 4-byte BE length + JSON, serde-tagged `DebugRequest`; no version or handshake field, so engine and `byro-dbg` must come from one workspace commit. Guards: `round_trip_request`, `round_trip_response` (`wire.rs`).
- A new `DebugRequest` / `DebugResponse` variant must reach the evaluator dispatch, `parse_shorthand` and `display.rs` in `byro-dbg`, and `handle_response` / `variant_name` in `tui.rs`; a `_ =>` arm in any of them hides a missing variant. `byro-dbg` sets a 10 s socket read timeout (`configure_read_timeout`, guard `configures_a_ten_second_read_timeout`) and its only other test is `parses_common_shorthand_requests` — variant coverage in `display.rs` / `tui.rs` is untested.
- Console docs vs the dispatch table (`registry.register(` in `byroredux/src/commands/mod.rs`): every documented command exists; every registered one is documented or deliberately omitted. `set_field` never panics on bad JSON.

### Dimension 3: SDK contract surface
Paths: `crates/sdk/src/`, `byroredux/src/{studio_host.rs,commands/studio.rs}`, `crates/debug-ui/src/panels.rs`, the two SDK plan docs.
First step: `cargo tree -p byroredux-sdk --depth 1` and `grep -rn 'byroredux_core::' crates/sdk/src`.
- Facts (2026-09-19): `#![forbid(unsafe_code)]`; deps are `byroredux-core` (only `math::Vec3` used), `semver`, `serde`, `thiserror`; `StudioSession` is `pub(crate)` in `byroredux/src/studio_host.rs`, not the SDK.
- Plan gates: an explicit checked dependency list (no renderer/UI/platform/archive/Wasmtime), "Rust/WIT schema drift fails CI" (`crates/mod-runtime/wit/host.wit`), warning-free docs, and "no supported SDK value exposes an ECS ID or engine pointer". Find the test or CI step for each (none found for the dependency list, 2026-09-19). The plan is `In progress`: report only gates the docs call *delivered*.
- API leak: a `pub` SDK item exposing an engine type (`Entity`, `World`, `FormId`, glam beyond `Vec3`) instead of a stable id (`ObjectId`, `AssetId`, `EntityRef`, `FormRef`); `pub` value-type fields are unvalidated by construction.
- Validation lives in the host: `valid_transform` / `valid_material` in `studio_host.rs` reject non-finite values, while the clamps (`sanitize_transform`, `sanitize_material`) sit in the egui panel; every non-UI command path (extensions, `commands/studio.rs`) must reach the host validators.
- Snapshot determinism: ids come from a sorted canonical set (plan Phase 1); check no `HashMap` iteration order reaches a `StudioSnapshot` or serialized output. Identity guards: `object_ids_reserve_zero_and_assign_one_based_ordinals`, `namespaced_ids_reject_path_and_whitespace_spoofing`, `capability_sets_are_validated_and_deduplicated`.
- Every `*_CAPABILITY` constant in `service.rs` is checked at a real call site (count by grep); an unchecked one is a grant without a gate (enforcement is `/audit-safety` Dim 8).
- StorageUtil / list verbs (`crates/sdk/src/compatibility/storage_util/` — `mod.rs` scalar + codecs, `list.rs`, `declarations.rs`): bounded sizes and deterministic order per the plan's compatibility table; every provider alias maps to a route, policy and fixture; the unsupported boundary (file-backed state, JContainers JSON/Lua, menu mutation) fails explicitly, never no-ops.

### Dimension 4: Launcher boot handoff and persistence
Paths: `crates/boot-request/src/`, `crates/settings-io/src/`, `crates/core/src/atomic_file.rs`, `crates/game-detect/src/overrides.rs`, `tools/byro-launcher/src/`, `byroredux/src/boot/cli.rs`.
First step: `grep -n 'fs::write\|atomic_write\|CONTRACT_VERSION\|SETTINGS_VERSION' crates/boot-request/src crates/settings-io/src crates/game-detect/src tools/byro-launcher/src`.
Guards: version — `an_unknown_contract_version_is_refused_by_version_not_by_field`, `a_missing_version_is_a_mismatch_not_a_default`; precedence (`docs/engine/launcher.md` §2.4) — `explicit_command_line_flags_suppress_their_request_counterparts`, `boot_request_seam_tests` in `boot/cli.rs`; settings — `saving_a_partial_registry_preserves_keys_it_does_not_know`, `stale_and_invalid_values_do_not_block_valid_siblings`, `the_shipped_presets_apply_cleanly_to_the_real_builtin_registry`; supervision — `the_engine_is_invoked_with_the_boot_request_path`, `the_log_tail_is_bounded_and_keeps_the_most_recent_lines`.
Checklist:
- `atomic_write` (temp -> fsync -> read-back -> rename -> parent fsync, #3472) is the durability contract. All three writers now use it (2026-09-29): `BootRequest::save` (`boot-request` depends on core), `RootOverrides::merge_into_file`, settings-io `save_to_path`; guards `save_uses_the_shared_atomic_file_writer`, `merge_uses_the_shared_atomic_file_writer`. Each shares one fallback: on *any* rename error with an existing destination it does a bare `fs::write` over it (commented as Windows-only, not gated to it) — non-atomic. `merge_into_file` still round-trips the user's hand-edited *profiles.toml* through `toml::Table`, dropping comments. Severity by data at risk: user-edited profiles MEDIUM, per-Play boot.toml LOW.
- Two version policies: `boot-request` refuses a mismatch; `settings-io` logs, "attempts compatible values", then re-saves stamped version 1 (a newer file is silently downgraded). Intended?
- Lost updates: launcher and engine both read-modify-write *settings.toml* with no lock, and a file that fails to parse is swallowed (`unwrap_or_default` in `save_to_path`), so the next save drops the user's unknown keys (input bindings) — does it log?
- Precedence: settings path is `SETTINGS_PATH_ENV`, then the platform config dir, then a cwd fallback (`discover_settings_path`); games root is `--games-root`, `BYROREDUX_GAMES_ROOT`, then `DEFAULT_GAMES_ROOT`, one developer's path (`crates/game-detect/src/profiles.rs`) — a player's machine silently resolves to a missing directory; does anything say so? boot.toml paths are absolute and never `~`-expanded (`docs/engine/launcher.md` §2.2) while `profiles.rs` expands `~/` on roots — check they agree.
- The launcher must run when the engine cannot (no Vulkan): `cargo tree -p byro-launcher` may show `ash` (pre-flight probe) but not the renderer or `byroredux-ui`; `a_directory_with_the_engine_name_is_not_the_engine` guards `locate_engine`.

### Dimension 5: Install detection and profile registry
Paths: `crates/game-detect/src/{catalog,steam,profiles,validate}.rs`, `crates/core/src/ecs/game_profiles.rs`, `byroredux/src/game_profiles.rs` (re-export shim), `assets/debug_profiles.toml`, `tools/byro-detect/src/main.rs`.
First step: `cargo test -p byroredux-game-detect` and `git log --since=<last report> -- crates/game-detect assets/debug_profiles.toml`.
- Steam only today (`~/.steam/*`, `~/.local/share/Steam`, Flatpak, macOS, Windows `ProgramFiles*`; the registry probe is marked "still to do"); GOG/Epic and Wine prefixes are not probed — a Wine install is reachable only via a manual `[roots]` entry, which `byro-detect` says when nothing is found.
- Alternate releases (`[[profiles.<key>.releases]]`, chosen by files on disk via `GameProfileEntry::for_data_dir`): guards `an_alternate_release_is_validated_against_its_own_archives`, `the_primary_release_wins_when_its_archives_are_all_present`, `a_release_replaces_only_the_lists_it_names`. The launcher validator, the engine's `expand_game_profile_args` (`byroredux/src/boot/cli.rs`) and `studio_host.rs` must all go through `for_data_dir`, not a private copy; sibling rules come from `byroredux_bsa::numeric_sibling_paths`.
- Env-var spelling: canonical is `BYROREDUX_<GAME>_DATA` (`crates/plugin/src/esm/test_paths.rs`, e.g. `BYROREDUX_SKYRIMSE_DATA`). `63c0aee3b` (Session 91) unified the Skyrim SE split; the legacy *BYROREDUX_SKYRIM_DATA* now survives only as a repo-variable fallback in `.github/workflows/playable-smoke.yml` (#4760 is still open though its Rust/script readers are gone). A new reader of the old name is a regression of that unification (#3741 was the earlier same-class failure).
- `validate.rs`: a zero-byte or truncated `.esm` / archive must fail at the right `Severity`, not pass and then abort the engine.
- `byro-detect`: exit failure when nothing is found, text output only, `--write` records `[roots]` via the non-atomic merge (Dim 4); unknown flags are silently ignored (`args.iter().any`) — check `--help` matches behaviour.

### Dimension 6: Tool CLIs
Paths: `tools/texture-upscale/src/`.
First step: `cargo test -p byro-texture-upscale`.
- texture-upscale (`MANIFEST_VERSION` 1, `Manifest::validate`): `output_png_path` follows `validate_asset_path` (rejects absolute, `.`, `..`, empty segments); overwrite refused without `--overwrite`; `--dry-run` creates nothing (`dry_run_checks_the_plan_without_creating_output_or_launching_the_model`); dimensions overflow-checked (`scaled_dimensions_reject_overflow`); disk space checked first (`insufficient_space_is_rejected_before_writes`).
- Trust: the upscaler is `Command::new(&manifest.upscaler.program)` with `.args(..)` (no shell), so the manifest is executable trusted input; README/`--help` must say so.
- Output collisions and per-set failure are fixed (#4761/#4762): `validate` claims each source's output PNG across every set (`manifest_rejects_sources_that_share_an_output_png`); a bad set is reported, not batch-fatal (`one_bad_set_does_not_abort_the_batch`), while an unlaunchable upscaler still aborts (`an_unlaunchable_upscaler_still_aborts_the_batch`). Check: `SourceStack` "later overrides earlier" matches the engine's last-wins order (`/audit-parsers` Dim 6).

## Phase 3: Merge and Cleanup

Write `docs/audits/AUDIT_TOOLING_<TODAY>.md`: Executive Summary (findings by severity, areas swept), deduplicated Findings, and an Exposure Matrix (surface x {reachable by, authenticated, size-capped, persistence atomic}). Cross-audit dedup: debug-server locks `/audit-concurrency` Dim 7; sandbox enforcement `/audit-safety` Dim 8; save schema `/audit-save`; VDF/archive parsing `/audit-parsers`. Then `rm -rf /tmp/audit/tooling` and suggest `/audit-publish docs/audits/AUDIT_TOOLING_<TODAY>.md` (labels: `tech-debt` for debug-server/launcher/tools, which have no label of their own; `doc-rot`, `test-gap` as fitting).
