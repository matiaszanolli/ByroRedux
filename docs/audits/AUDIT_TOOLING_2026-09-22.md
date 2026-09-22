**HEAD**: ee6d3fb39 · **Baseline**: none (first run) · **Audited**: Dim 1
(Debug Trust Boundary), Dim 2 (Protocol & Registry Completeness), Dim 3 (SDK
Contract Surface), Dim 4 (Boot Handoff & Persistence), Dim 5 (Install
Detection), Dim 6 (Tool CLIs) · **Unchanged since baseline (skimmed)**: none
(no prior `AUDIT_TOOLING_*.md` exists, so this is a full run per
`_audit-common.md`)

# Tooling and Host-Contract Audit — 2026-09-22

Scope: `crates/sdk/`, `crates/debug-server/`, `crates/debug-protocol/`,
`crates/debug-ui/`, `crates/boot-request/`, `crates/settings-io/`,
`crates/game-detect/`, `tools/byro-launcher/`, `tools/byro-detect/`,
`tools/texture-upscale/`, `tools/byro-dbg/`, and the engine-side adapters at
`byroredux/src/{studio_host.rs,commands/studio.rs,extensions/,scene.rs,
app_events.rs,boot/cli.rs}`. No sub-agents were used; every dimension was
analysed directly, one at a time, writing findings to
`/tmp/audit/tooling/dim_<N>.md` before starting the next (all six scratch
files exist).

## Test baseline

`cargo test -j 4` for every tooling crate — all green, no failures:

| Crate | Tests |
|---|---|
| `byroredux-sdk` | 104 passed |
| `byroredux-debug-server` | 16 passed |
| `byroredux-debug-protocol` | 6 passed |
| `byroredux-debug-ui` | 16 passed |
| `byroredux-boot-request` | 16 passed |
| `byroredux-settings-io` | 8 passed |
| `byroredux-game-detect` | 45 passed |
| `byro-launcher` | 24 passed, 1 ignored (`needs a Vulkan driver`) |
| `byro-detect` | 0 (no unit tests) |
| `byro-texture-upscale` | 15 passed (lib), 0 (bin) |
| `byro-dbg` | 0 (no unit tests — itself a finding, see TOOL-D1-2026-09-22-03) |

## Executive Summary

11 findings, all NEW (none matched an existing open issue; the refreshed
`/tmp/audit/issues.json` — `gh issue list --limit 6000 --state all` — was
searched by keyword per finding before filing, and again at the end for a
final sweep). Severity breakdown: **1 HIGH, 6 MEDIUM, 4 LOW, 0 CRITICAL**.

The one HIGH finding is the headline: the debug TCP port is on by default in
every build (including `--release`), has zero authentication, and grants
unauthenticated local processes arbitrary filesystem read/write as the
engine's user through three independent request paths (`Screenshot`,
`tex.dump`, `LoadNif`). Everything else is MEDIUM/LOW-grade completeness,
doc-rot, and robustness gaps typical of a pre-1.0 developer-tooling surface.

Per the task brief, `AUDIT_UI_2026-09-21.md`'s `UI.IsMenuOpen` pointer was
independently confirmed and filed as TOOL-D3-2026-09-22-01, with the other
SDK bridge call sites checked for the same pattern (none found elsewhere).
`AUDIT_SAFETY_2026-09-21.md`'s SAFE-D4-01/SAFE-D5-01 (CI clippy/validation
gates, now #4595/#4596) were reviewed for a tooling-side consequence; none
was found distinct from the safety audit's own framing, so they are not
re-filed here.

Areas swept: debug-server trust boundary and wire protocol; component/
console-command registry completeness vs. docs; SDK dependency/API-leak
surface, capability-constant enforcement (cross-repo, corrected an initial
narrow-scope false positive before filing), Studio command validation
choke-point; boot-request/settings-io/game-detect-overrides persistence
durability and version policy; Steam-only install detection, ESM/archive
pre-flight validation, env-var naming; texture-upscale manifest validation,
output-path collision safety, and batch-failure handling.

## Findings

### TOOL-D1-2026-09-22-01: Unauthenticated debug port grants arbitrary local file read + write, on by default in every build including `--release`
- **Severity**: HIGH
- **Dimension**: Debug Trust Boundary
- **Exposure**: any local process on the machine (no auth); reachable in a
  stock `cargo run` / `cargo build --release` binary, not just a dev profile
- **Location**: `crates/debug-server/src/system.rs:90,105` (Screenshot
  write); `byroredux/src/commands/assets.rs:241-296` (`tex.dump` read+write);
  `byroredux/src/debug_load.rs:11-14,49` (`LoadNif` read); `byroredux/
  Cargo.toml:8-9`; `byroredux/src/main.rs:816-822`
- **Status**: NEW
- **Description**: `byroredux/Cargo.toml:8-9` sets `default =
  ["debug-server"]`, and `main.rs:816-822` unconditionally starts the debug
  TCP listener (default port 9876) behind only that always-on feature.
  `crates/debug-server/src/listener.rs:173` binds `("127.0.0.1", port)` with
  zero authentication of any kind — any local process can connect. Three
  independent request paths then let that connection touch the filesystem:
  `DebugRequest::Screenshot { path }` calls `std::fs::write(path,
  &png_bytes)` with the client-chosen path verbatim (no root confinement, no
  extension check); the console command `tex.dump <bsa-path> <texture-path>
  [out.png]` (reachable via `Eval`, since `eval_request` dispatches any
  `CommandRegistry` name) opens `archive_path` verbatim and writes the
  decoded PNG to a client-chosen `out_path`, independently of Screenshot;
  `DebugRequest::LoadNif { path, .. }` opens and parses any filesystem path
  the client supplies (`debug_load.rs`'s own doc comment: "try a loose
  absolute path first").
- **Evidence**:
  ```rust
  // crates/debug-server/src/system.rs:90
  Some(path) => match std::fs::write(path, &png_bytes) { ... }
  ```
  ```rust
  // byroredux/src/commands/assets.rs:249-256
  let out_path = parts.get(2).cloned().unwrap_or_else(|| "/tmp/tex_dump.png".to_string());
  let archive = match crate::asset_provider::Archive::open(archive_path) { ... }
  ```
- **Impact**: A local, otherwise-unprivileged process on the same machine
  (any other application running as the same user, or anything that can
  reach a fixed, well-known default TCP port) can overwrite arbitrary files
  reachable by the engine process, with no opt-in beyond "the engine is
  running". This escapes the ECS/world sandbox entirely onto the host
  filesystem — worse than the severity table's explicit HIGH-floor row for
  "network-reachable debug/console surface that mutates world state without
  an explicit opt-in".
- **Related**: #3449 (closed) addressed connection/thread exhaustion caps,
  not this. No existing issue covers arbitrary file write/read via
  Screenshot or tex.dump. Different surface from `/audit-safety` Dim 8's
  mod-runtime WASM sandbox.
- **Suggested Fix**: Require an explicit opt-in (env var or CLI flag) to
  start the debug server in a release build, keeping default-on only for
  dev builds via a `dev` feature; and/or confine `Screenshot`/`tex.dump`
  output paths to a fixed screenshots/dumps directory (reject absolute
  paths and `..`), and confine `tex.dump`'s archive-open and `LoadNif`'s
  path to the configured game-data root(s).

### TOOL-D1-2026-09-22-02: `WalkEntity.max_depth` and generic dump responses are unbounded on the encode side, while the decode side enforces a 16MB cap — an oversized response silently kills the byro-dbg connection
- **Severity**: MEDIUM
- **Dimension**: Debug Trust Boundary
- **Exposure**: developer-only (byro-dbg operator), also reachable from any
  local process per TOOL-D1-2026-09-22-01's Exposure
- **Location**: `crates/debug-protocol/src/wire.rs:12-20` (`encode`, no size
  check against `MAX_MESSAGE_SIZE`); `crates/debug-server/src/
  evaluator.rs:333-395` (`eval_walk_entity` — client-supplied `max_depth:
  u32` with no cap, no visited-set); `tools/byro-dbg/src/main.rs:91-97` and
  `tools/byro-dbg/src/tui.rs:394-402` (both terminate the connection on any
  decode error, including "message too large")
- **Status**: NEW
- **Description**: `wire::encode` writes `let len = json.len() as u32;`
  with no check against `MAX_MESSAGE_SIZE` — that constant is only consulted
  on the *decode* side. `WalkEntity`'s `max_depth` has no upper bound and
  the walk keeps no visited-entity set, so a large hierarchy at a high
  `max_depth` (or `ListEntities` on a densely-populated, densely-named
  world) can produce a response over the 16MB cap the client enforces on
  read. When that happens the client's `decode` rejects the frame *before*
  reading the payload and returns an error; both byro-dbg entry points treat
  any decode error as fatal (REPL `break`s and effectively exits; TUI
  net-thread `break`s, silently freezing the dashboard with no reconnect).
- **Evidence**:
  ```rust
  // crates/debug-protocol/src/wire.rs:12-19
  pub fn encode<T: Serialize>(msg: &T) -> io::Result<Vec<u8>> {
      let json = serde_json::to_vec(msg)...;
      let len = json.len() as u32;   // no MAX_MESSAGE_SIZE check
  ```
- **Impact**: A single `walk <root> 999999` against a real cell (or any
  command against a large world) can kill the whole `byro-dbg` session with
  an opaque "message too large" error; the TUI variant degrades silently
  (net thread dies, dashboard freezes, nothing tells the operator why).
  Developer-tool defect only, fully recoverable by reconnecting — MEDIUM per
  the Exposure guidance's "developer-only tool defect = LOW–MEDIUM", taken
  at the upper end because the TUI failure mode is silent.
- **Suggested Fix**: Cap `max_depth` server-side (e.g. 64) and add a
  visited `HashSet<u32>` to `eval_walk_entity`; have the server synthesize a
  `DebugResponse::error(...)` instead of sending an over-cap frame when the
  encoded payload would exceed `MAX_MESSAGE_SIZE`.

### TOOL-D1-2026-09-22-03: `byro-dbg` has no socket read timeout and no `#[test]` in the whole crate
- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: developer-only tool
- **Location**: `tools/byro-dbg/src/main.rs` (no `set_read_timeout` call
  anywhere in the crate); confirmed via `cargo test -p byro-dbg` → 0 tests
- **Status**: NEW
- **Description**: The server always answers within ~5s independent of the
  frame loop (`listener.rs:362`'s `rx.recv_timeout`), so an ECS-side stall
  alone does not hang the client. The narrower but real trigger is the
  engine *process* itself being fully wedged (OS-level freeze, debugger
  attach, severe thrashing) — in that case `wire::decode`'s `read_exact`
  blocks with no client-side timeout, hanging the REPL/TUI indefinitely.
  Separately, the entire 1198-LOC crate has zero `#[test]` functions —
  `parse_shorthand`, `display::print_response`, and the TUI's
  `handle_response`/`variant_name` are all unit-testable with no coverage.
- **Impact**: Narrow trigger (full process freeze, not a slow query), so
  LOW; paired with a total absence of test coverage on a hand-parsed
  dispatcher.
- **Suggested Fix**: Add `stream.set_read_timeout(Some(Duration::
  from_secs(10)))` before the request loop; add unit tests for
  `parse_shorthand` at minimum.

### TOOL-D2-2026-09-22-01: `ActorValues`/`ActorVitals` and five other core gameplay components implement `Component` + inspect-serialize but are never registered — `inspect <id>` and `entities <Component>` cannot see them
- **Severity**: MEDIUM
- **Dimension**: Protocol & Registry Completeness
- **Location**: `crates/core/src/ecs/components/actor_values.rs:67-170`
  (`ActorValues`, `ActorVitals`); `crates/core/src/ecs/components/
  inventory.rs:160` (`EquippedWeapon`); `crates/core/src/ecs/components/
  actor_state.rs:18` (`Dead`); `crates/core/src/ecs/components/
  creature_attack.rs:45` (`CreatureAttack`); `crates/core/src/character/
  components.rs:132,255` (`Perks`, `FactionReputation`); none appear in
  `crates/debug-server/src/registration.rs`'s `register_all`
- **Status**: NEW
- **Description**: `ActorValues` is, per its own module doc, "the
  production store... shared by every gameplay reader" — the backing for
  `GetActorValue`, health/SPECIAL/skills. Both it and `ActorVitals` are real
  `SparseSetStorage` components with the `inspect` derive already present —
  simply never added to `register_all`. This is the same gap pattern #4063
  fixed for the six AI-procedure `*Behavior` components, recurring for
  components more central to routine gameplay debugging than any AI
  procedure. The only existing access path is write-only-by-formid
  (`setav`/`modav`) or one-value-at-a-time (`cond`) — there is no generic
  dump, unlike `Inventory`/`EquipmentSlots` (explicitly wired for the M41
  smoke test). Same gap covers `EquippedWeapon`, `Dead`, `CreatureAttack`,
  `Perks`, `FactionReputation`.
- **Evidence**: `grep -n 'ActorValues\|ActorVitals\|EquippedWeapon\|
  CreatureAttack' crates/debug-server/src/registration.rs` → no matches,
  against 42 confirmed `register_component::<T>()` call sites total.
- **Impact**: "Why is this NPC's health/skill/SPECIAL wrong" has no generic
  byro-dbg path. The existing completeness guard
  (`roster_tests::every_ai_procedure_behavior_component_is_registered`)
  only derives its roster from `impl Component for <X>Behavior` and
  structurally cannot catch this class of gap.
- **Related**: Existing: #4063 (closed) — same gap pattern, different
  component family. New instance, not a regression.
- **Suggested Fix**: Register the seven listed components in `register_all`
  (mirrors the M41 `Inventory`/`EquipmentSlots` precedent in the same
  function). Widen `roster_tests`'s guard from "every `*Behavior` type" to
  "every `impl Component` type carrying the `inspect` derive", so future
  gaps in this class fail a test instead of landing silently.

### TOOL-D2-2026-09-22-02: `docs/engine/debug-cli.md`'s two registration counts are both stale, and 15 of 92 console commands have no documentation row anywhere in the file
- **Severity**: LOW
- **Dimension**: Protocol & Registry Completeness
- **Location**: `docs/engine/debug-cli.md:179` ("23 components" vs. live
  42); `docs/engine/debug-cli.md:276` ("68" commands vs. live 92);
  `byroredux/src/commands/hud.rs` (6 undocumented `hud.*` commands),
  `byroredux/src/commands/{depth,gameplay,scene,ragdoll_status,
  world_info,quest}.rs` (9 more)
- **Status**: NEW
- **Description**: `grep -c 'register_component::<'
  crates/debug-server/src/registration.rs` = 42 vs. the doc's "23".
  `grep -c 'registry.register(' byroredux/src/commands/mod.rs` = 92 vs. the
  doc's "68". Cross-referencing all 92 command names (by exact `fn
  name(&self)` string) against the full text of the doc (fixed-string
  search, not just the summary block) finds **15 with zero mention
  anywhere**: the entire HUD family (`hud.on`, `hud.off`, `hud.values`,
  `hud.heading`, `hud.status`, `hud.debug`), `hardcore` (a gameplay mode
  toggle), `exposure`/`tonemap` (render debug controls), `depth.stats`,
  `npc.appearance`, `ragdoll.status`, `sdk.compat`, `rt.masks`,
  `cell.owners`, `quest.effects`. No documented command is missing from the
  registry (drift is one-directional).
- **Impact**: Discoverability only — `help` lists everything live — but a
  developer reading the doc to learn the surface will not find these 15
  commands.
- **Suggested Fix**: Regenerate both counts and add rows for the 15 missing
  commands; consider a doc-drift CI guard that asserts the doc mentions
  every `CommandRegistry`/`ComponentRegistry` name (this exact pattern hit
  `TD4-2026-08-27-05`, `TD4-2026-09-05-01`, `SCR-ORCH-2026-09-06-01` in
  other docs this cycle).

### TOOL-D3-2026-09-22-01: `UI.IsMenuOpen` always returns `false` for every real Bethesda menu name — confirms and files the `/audit-ui` 2026-09-21 pointer
- **Severity**: MEDIUM
- **Dimension**: SDK Contract Surface
- **Location**: `crates/sdk/src/compatibility/input_ui.rs:142-149`
  (`adapt_papyrus_ui_is_menu_open` — the comparison itself is not buggy);
  `byroredux/src/app_events.rs:546-550` (populates the snapshot from
  `ui.menu_name.as_str()`); `crates/ui/src/lib.rs:64,144`
  (`UiManager::menu_name`); `byroredux/src/scene.rs:1292` (`--menu`
  production route) and `:1363` (`--swf` dev route) — both pass the
  archive/file path as the menu name
- **Status**: NEW (confirms `AUDIT_UI_2026-09-21.md` § 6 item 4, which
  pointed here explicitly: "Pointer to `/audit-tooling`")
- **Description**: `UiManager::menu_name` is set from the on-disk movie
  path (`interface\hudmenu.swf`-shaped) on **both** the `--menu` archive
  route and the `--swf` dev route — checked both call sites directly, not
  just one. No canonical Bethesda-menu-name table exists anywhere in
  `crates/ui` (checked `catalog.rs`, `profile.rs`, `lib.rs`). The declared,
  dispatched SDK route `UI.IsMenuOpen("InventoryMenu")` therefore compares
  a real Bethesda name against an archive path and can never match.
- **Evidence**:
  ```rust
  // byroredux/src/app_events.rs:546-550
  crate::extensions::extension_ui_menu_sync(&self.world, Some(ui.menu_name.as_str()), ui.visible);
  ```
  ```rust
  // byroredux/src/scene.rs:1292 (--menu, production route)
  ui.load_swf_from_resource_provider(Arc::new(archive), menu_path, menu_path, None)
  ```
- **Impact**: Zero real-world blast radius today (the UI audit already
  scopes this under "Pending-Row Readiness" — no shipped script relies on
  it) — but the function is fully wired end-to-end (declared, routed,
  dispatched, unit-tested with synthetic names) and would silently
  misbehave for every real caller the moment a compat script uses it.
  Checked other SDK bridge call sites in `install.rs`'s dispatch for the
  same pattern: the `Game.Get*Mod*` adapters read real plugin filenames
  from `ContentCatalog`, and the `Input` adapters compare against the real
  action-binding table — no other instance found.
- **Related**: `AUDIT_UI_2026-09-21.md` § 6 item 4.
- **Suggested Fix**: Add a vanilla-menu-name lookup (SWF basename,
  case-insensitive, minus extension → canonical Bethesda name) at the
  `UiManager`/`install_player` boundary or in `extension_ui_menu_sync`
  before constructing the snapshot; add a test that loads a real archive
  menu path and asserts `UI.IsMenuOpen("HUDMenu")` resolves `true`.

### TOOL-D4-2026-09-22-01: `BootRequest::save` and `RootOverrides::merge_into_file` still use bare `fs::write`, one crate over from the project's own atomic-write doctrine
- **Severity**: MEDIUM (profiles.toml path) / LOW (boot.toml path)
- **Dimension**: Boot Handoff & Persistence
- **Location**: `crates/boot-request/src/lib.rs:293-309`
  (`BootRequest::save`); `crates/game-detect/src/overrides.rs:96-136`
  (`RootOverrides::merge_into_file`)
- **Status**: NEW
- **Description**: `atomic_write` (temp → fsync → read-back → rename →
  parent fsync, `crates/core/src/atomic_file.rs`) is the project's
  documented durability contract; `settings-io` was moved onto it by #3472
  specifically because "there is no reason for two writers in one binary to
  have two different durability contracts." Two more writers remain
  un-migrated: `BootRequest::save` (writes the launcher→engine `boot.toml`
  handoff) and `RootOverrides::merge_into_file` (rewrites the user's
  hand-edited `~/.byroredux/profiles.toml`, merge-preserving hand-authored
  `[profiles.*]`/`[defaults]` blocks per its own doc comment).
  `boot-request` deliberately has no `byroredux-core` dependency (confirmed
  via `cargo tree`), so it cannot call `atomic_write` without a dependency
  decision.
- **Evidence**:
  ```rust
  // crates/boot-request/src/lib.rs:307
  fs::write(path, text).map_err(|source| BootRequestError::Write { ... })
  ```
  ```rust
  // crates/game-detect/src/overrides.rs:133
  std::fs::write(path, text).map_err(|source| OverrideError::Write { ... })
  ```
- **Impact**: A crash/power-loss mid-write leaves a truncated file. For
  `boot.toml`, low value at risk (the launcher regenerates it fresh every
  `Play`, so a torn file only fails the *next* attempt with a clear parse
  error). For `profiles.toml`, the file carries hand-authored user content
  a torn `byro-detect --write` could destroy alongside the freshly-detected
  `[roots]` table.
- **Related**: #3472 (closed) fixed this exact class for `settings-io`;
  its own rename-failure fallback (`fs::write` over the destination on
  Windows rename failure) is separately, deliberately non-atomic per an
  in-place comment — not re-filed.
- **Suggested Fix**: Add `byroredux-core` as a dependency of `boot-request`
  and reuse `atomic_write` directly, or extract it into a dependency-light
  shared crate both `boot-request` and `game-detect` can use.

### TOOL-D5-2026-09-22-01: `validate()` never structurally checks the main plugin (`.esm`) — a truncated/corrupt ESM reports `Severity::Ok`, while the identical failure mode on an archive is correctly caught
- **Severity**: MEDIUM
- **Dimension**: Install Detection
- **Location**: `crates/game-detect/src/validate.rs:143-163` (ESM check —
  existence + byte-count only) vs. `:165-206`/`:100-114` (archive check —
  real `BsaArchive::open`/`Ba2Archive::open`)
- **Status**: NEW
- **Description**: The archive-validation loop explicitly reasons that "a
  truncated download or a mod manager that mangled the file... the engine
  would fail mid-load rather than at startup" and calls the real archive
  header parser accordingly, reporting `Severity::Fail` on a parse error.
  The main-plugin check applies none of that reasoning: `esm.is_file()`
  plus a byte-count-as-MB informational `Severity::Ok` line, with no
  attempt to open or structurally validate the ESM. Even the crate's own
  test fixtures never exercise a real header here — every fixture writes
  the literal 4 bytes `b"TES4"` with no further structure.
- **Evidence**:
  ```rust
  // crates/game-detect/src/validate.rs:157-162 — size only, never opened
  let size_mb = esm.metadata().map(|m| m.len()).unwrap_or(0) / (1024 * 1024);
  checks.push(Check::new(Severity::Ok, "Main plugin", format!("{} ({size_mb} MB)", entry.esm)));
  ```
- **Impact**: `byro-detect`/the launcher pre-flight reports a broken
  install as "ready" when the corruption is in the ESM rather than an
  archive, defeating the tool's stated purpose. `crates/plugin` already has
  an ESM header parser available — this is a wiring gap, not a missing
  capability.
- **Suggested Fix**: Call the existing ESM header parser (or at minimum
  check the leading `TES4`/`TES3` magic and a sane minimum size) in the
  main-plugin branch, mirroring the archive branch's `Severity::Fail` on
  parse error.

### TOOL-D5-2026-09-22-02: `BYROREDUX_SKYRIM_DATA` (not the documented canonical `BYROREDUX_SKYRIMSE_DATA`) is still the only env var five Rust files and nine shell scripts read for Skyrim SE data
- **Severity**: LOW
- **Dimension**: Install Detection
- **Location**: `byroredux/src/asset_provider/animation.rs:821,913`;
  `byroredux/src/npc_spawn/ai_package.rs:1558`; `crates/hkx/src/
  animation.rs:1234,1304`; `crates/scripting/examples/
  mq101_conformance.rs:15,428,436`; plus nine `docs/smoke-tests/*.sh` /
  `scripts/*.sh`. Canonical: `crates/plugin/src/esm/test_paths.rs:83`
  (`SKYRIM_SE_ENV = "BYROREDUX_SKYRIMSE_DATA"`)
- **Status**: NEW (re-verified still true at HEAD; not covered by any open
  issue after a fresh keyword search of the refreshed issue cache)
- **Description**: Every site is opt-in test/tooling code — `#[ignore]`
  -gated `#[test]` functions or a standalone `cargo run --example` probe /
  shell smoke scripts — never the running engine or a player-facing code
  path (checked each of the five `.rs` sites directly). A developer who
  configures the documented canonical `BYROREDUX_SKYRIMSE_DATA` will find
  these specific opt-in tests/scripts silently fall back to a hardcoded
  path that only resolves on this repo's own dev machine.
- **Impact**: Confined to opt-in developer workflows; never a player or
  production-engine defect.
- **Related**: #3741 (closed) addressed helper visibility, not this
  spelling split — not a regression of it.
- **Suggested Fix**: Rename all five `.rs` sites (ideally by importing
  `SKYRIM_SE_ENV` instead of a string literal, so the existing completeness
  guard covers them) and the nine shell scripts.

### TOOL-D6-2026-09-22-01: Neither `Manifest::validate` nor `preflight_outputs` dedupes by the *derived* output path — two source assets differing only by extension silently collide on one `.png`
- **Severity**: MEDIUM
- **Dimension**: Tool CLIs
- **Location**: `tools/texture-upscale/src/lib.rs:76-104`
  (`Manifest::validate`'s dedup — keys on the normalized *source* string,
  extension included); `lib.rs:201-210` (`output_png_path` —
  `.set_extension("png")` discards the source extension);
  `pipeline.rs:214-234` (`preflight_outputs` — checks each output against
  on-disk existence independently, never against sibling outputs in the
  same run)
- **Status**: NEW
- **Description**: `output_png_path` replaces whatever extension a source
  path had with `.png`, so two source paths identical except for extension
  collapse to one output file. `Manifest::validate`'s "map path duplicates
  the reference" check and its per-set dedup set both compare normalized
  *source* strings (extension included), so e.g. `reference:
  "textures/wood.dds"` + `maps: [{path: "textures/wood.tga"}]` in the
  **same** `TextureSet` passes validation despite both resolving to
  `textures/wood.png`. `preflight_outputs` checks each computed output
  against disk state but never accumulates outputs to check against each
  other, so cross-set collisions pass too when neither destination
  pre-exists yet.
- **Evidence**:
  ```rust
  // lib.rs:92-98 — compares source strings including extension
  if map_path == normalize_asset_path(&set.reference) { bail!(...) }  // "wood.tga" != "wood.dds"
  ```
- **Impact**: Default (`--overwrite` off): the first colliding write
  succeeds; the second fails mid-batch via a fresh `path.exists()` check,
  aborting the whole run *after* the expensive external upscaler already
  ran for earlier, unrelated sets. `--overwrite` on: the second colliding
  write silently replaces the first with no warning and no `RunReport`
  entry — one of two logically distinct upscaled textures is silently
  lost.
- **Suggested Fix**: Accumulate every `output_png_path` result into a
  `HashSet` in `preflight_outputs` (or `Manifest::validate`) and `bail!` on
  a collision before any work begins, independent of `--overwrite` and
  on-disk state.

### TOOL-D6-2026-09-22-02: `run_manifest` aborts the whole batch on the first per-set failure despite `RunReport` being shaped for per-set results
- **Severity**: LOW
- **Dimension**: Tool CLIs
- **Location**: `tools/texture-upscale/src/pipeline.rs:85-90`
  (`sources.extract(...)?` / `decode_texture(...)?` inside the per-set
  loop, both propagating via `?`)
- **Status**: NEW
- **Description**: `decode_texture`'s BC5/BC7 error message is clear and
  well-worded, but it propagates via `?` out of `run_manifest` entirely —
  one unsupported texture anywhere in a large manifest aborts the run
  before any other, perfectly-decodable set is processed or reported on,
  even though `RunReport { sets: Vec<...> }`'s shape implies per-set
  results were the intent.
- **Impact**: A large batch job (the tool's core use case) loses all
  progress on the first bad entry rather than skipping and summarizing it.
  LOW — full clear error, non-zero exit, no data corruption.
- **Suggested Fix**: Collect a `Result` per set, push success/failure into
  `report.sets`, continue to the next set, and summarize failures at the
  end. Also worth a one-line README note that `[upscaler]` is trusted
  executable input (`Command::new`/`args`, confirmed no shell — not itself
  a vulnerability, just currently undocumented).

## Exposure Matrix

| Surface | Reachable by | Authenticated | Size-capped | Persistence atomic |
|---|---|---|---|---|
| Debug TCP server (`crates/debug-server`) | any local process (loopback bind, no auth) | No | Decode side only (16MB); encode side unbounded (TOOL-D1-02) | N/A (no persistent state of its own; Screenshot/tex.dump write arbitrary files — TOOL-D1-01) |
| SDK/extensions Papyrus-compat bridge (`crates/sdk`, `byroredux/src/extensions`) | in-process only (mod-runtime sandbox callers) | Capability-gated (all 28 `*_CAPABILITY` constants confirmed enforced in `crates/mod-runtime`) | N/A | N/A |
| `byroredux/src/studio_host.rs` (Studio commands) | in-process (egui panel, console `mat.*`/`studio.*`, future SDK paths) | Single validated choke point (`apply_command`) | N/A | N/A |
| Boot handoff (`crates/boot-request`) | launcher process only (writes), engine process only (reads) | N/A (local trust) | N/A | **No** — bare `fs::write` (TOOL-D4-01) |
| Settings (`crates/settings-io`) | launcher + engine, both local | N/A | N/A | Yes (`atomic_write`, #3472), except a deliberate Windows rename-failure fallback |
| Game-root overrides (`crates/game-detect::overrides`) | `byro-detect --write` / launcher, local | N/A | N/A | **No** — bare `fs::write` (TOOL-D4-01) |
| Install detection (`crates/game-detect`, `byro-detect`) | local CLI/launcher | N/A | N/A | Archives content-validated; ESM is not (TOOL-D5-01) |
| texture-upscale manifest execution | local CLI, operator-supplied manifest | N/A (manifest is trusted-executable input by design, undocumented — TOOL-D6-02) | Disk-space pre-checked before writes | Output-path collisions unguarded (TOOL-D6-01) |

## Cross-audit dedup notes

- Debug-server lock ordering: `/audit-concurrency` Dim 7 (not re-audited
  here).
- Mod-runtime sandbox enforcement of `crates/sdk`'s capability constants:
  `/audit-safety` Dim 8 — confirmed (cross-repo grep) every constant has a
  real enforcement call site there; not re-audited in depth.
- `AUDIT_SAFETY_2026-09-21.md`'s SAFE-D4-01/SAFE-D5-01 (CI clippy/Vulkan-
  validation gates broken, #4595/#4596): reviewed for a tooling-side
  consequence distinct from the safety framing; none found — both are
  CI-workflow-level gaps with no separate debug-server/SDK manifestation,
  so not re-filed here.
- Save-schema/format-version discipline: `/audit-save` (not re-audited).
- VDF/archive-parsing-specific byte-level robustness: `/audit-parsers`
  (not re-audited beyond the ESM-vs-archive validation *symmetry* gap in
  TOOL-D5-01, which is a game-detect pre-flight wiring issue, not a parser
  defect).

---

*Next run*: `/audit-publish docs/audits/AUDIT_TOOLING_2026-09-22.md`.
Suggested labels: `tech-debt` (debug-server/launcher/tools have no label of
their own), `doc-rot` (TOOL-D2-02), `test-gap` (TOOL-D1-03, TOOL-D6-02),
plus one severity label each.
