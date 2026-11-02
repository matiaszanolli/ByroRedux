# P2P Co-op — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (HEAD corrections to the determinism inventory and the script
budget; a third-party-server disposition for every connectivity route;
community-project claims limited to those projects' own documents; the VATS
PRD linked and Fallout 4's slow-motion lane covered; single-observable
splits; runnable acceptance; one home for the metrics). It turns the
ROADMAP's M60 row into a reviewable promise, inventories what at HEAD stands
between the engine and a shared session, and collects the owner's decisions
in §10. Nothing below is built yet. Statements about how an external
protocol, library, paper or community project behaves cite an external
source; a claim that could not be verified is marked **Phase 0: verify**
instead of being stated as fact.

**Scope**: Up to four people playing one Bethesda-game world together, one
peer acting as host, connected directly with no server that the project
operates or that the engine contacts by default. In: hosting and joining,
the replication model, several player characters in one world, scripts and
quests in a shared world, late join, session saves, host loss, and what each
single-player mechanic (pause, VATS, wait) becomes with more than one person
present. Out: matchmaking, server browsers, relays, accounts, telemetry,
anti-cheat, competitive PvP, voice, and any transfer of game or mod content
between peers.

**Related**: this document states the *promise*; the designs below own the
*how*. Two mechanisms have no owning design yet; Phase 0 writes both, as one
design.

| Mechanism | Owning design |
|---|---|
| Entities, storages, scheduler stages, declared access | [ECS](../engine/ecs.md) |
| Per-frame driver, delta time, pause and loading paths | [Game Loop](../engine/game-loop.md) |
| Fixed physics tick, Rapier bridge, character controller | [Physics](../engine/physics.md), [PHYSAL](../engine/physal.md) |
| Papyrus VM, ObScript runtime, frame budget, quest state | [Scripting](../engine/scripting.md) |
| Snapshot capture, validation gates, live load-apply | [Save/Load Round-Trip](../engine/save-load-roundtrip.md) |
| FormID-keyed identity across despawn and respawn | [Stream-boundary state continuity](../engine/stream-boundary-state-continuity.md) |
| Exterior streaming around the player | [Exterior Grid Streaming](../engine/exterior-grid-streaming.md) |
| Plugin identity, Form IDs, load order | [Plugin Loading](../engine/plugin-loading.md) |
| Extension sandbox, capability grants | [Sandboxed Linked Mods](../engine/sandboxed-linked-mods.md) |
| Host and join entry points, boot contract | [Launcher](../engine/launcher.md) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md) |
| VATS modes and the world time scale they publish | [VATS and Action Points PRD](vats.md) |
| Replication model, wire protocol, session lifecycle, host loss | none yet: Phase 0 writes `docs/engine/coop-replication.md` |
| Determinism analysis (§3.2, closed) | none yet: a section of the same design (§10 Q8) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `COOP-` prefix.

---

## 1. Product thesis

Where co-op exists for these games it is a community retrofit. Skyrim
Together's source tree is a Skyrim SE client ("engine hooks,
reverse-engineered game types and the sync logic"), a game server
(`STServer.dll`) and a dedicated-server executable that hosts it, under
GPLv3 ([TiltedEvolution README](https://github.com/tiltedphoques/TiltedEvolution)).
Its own feature list records the compromises: "Dialogue of NPCs is synced by
the party leader", "The party leader's client determines what weather is
currently active", "The AI of the follower is determined by who hosts that
follower", "You cannot progress the time by waiting or sleeping", and pausing
is disabled for several menus
([Tilted Online features](https://wiki.tiltedphoques.com/tilted-online/general-information/features.md)).
New Vegas has NV:MP, "an unofficial community-made multiplayer framework
modification for Fallout: New Vegas", built on the X:MP framework
([nv-mp.com](https://nv-mp.com/)). Whether every title in this lineage has
such a retrofit is not claimed here.

The ROADMAP's bet is that this engine does not need the workaround
([ROADMAP Tier 9](../../ROADMAP.md#tier-9--better-than-bethesda-capability-stretch-post-tier-4-horizon)):
world state is ECS data, saves are a validated snapshot, and scripts run on
a bounded runtime with no stack VM serialising itself into saves
([keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)).
The anti-scope fixes the shape of the answer: "engine ships the replication
layer, never a server"
([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).

The audience is a small group of friends who own the same game, run the same
mod profile, and want to play the single-player campaign together. They are
not strangers to be matched or a community to be hosted.

The corollary that shapes everything below: **the host's world is the game;
every other peer is a window into it.** Every rule (who runs scripts, whose
save it is, who can pause, what a quest's "player" means) follows from naming
one authority per session and making every other peer's view of it explicit.
The engine will not run a server, will not contact an address the user did
not enter, will not ship content between peers, and will not promise that a
mechanic built for one player (VATS, world pause) behaves as it did alone.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Session** | One shared game world with one host and up to three guests. Begins when the host opens it; ends when the last peer leaves, or when the host leaves and no migration happens. |
| **Peer** | One engine process taking part in a session. |
| **Host** | The peer whose `World` is authoritative. Its scheduler runs every simulation system; its save ring holds the session's saves. |
| **Guest** | Any peer that is not the host. Sends inputs, receives state, renders locally. |
| **Guest character** | The actor a guest controls inside the host's world. |
| **Authority** | The single peer whose value for a piece of state is the truth. Under the recommended model (§10 Q1) the host, for everything except each guest's own input. |
| **Input frame** | One peer's sampled control state for one simulation tick: the `ActionState` (`byroredux/src/interaction.rs`) plus look angles and the UI actions that change world state. |
| **Replicated state** | The subset of host ECS state a guest needs to render and predict, declared by a registry the way `SaveRegistry` declares saved state. |
| **Relevance set** | The replicated entities one guest receives, chosen by the host from that guest's position and UI. |
| **Content manifest** | The game masters, plugins, archives and extension packages a peer has loaded, each with a hash of its bytes. |
| **Checkpoint** | A full M45 snapshot the host sends to guests, for late join and host loss. |
| **Desync** | A guest's view of an authoritative value differing from the host's after the correction that should have fixed it. |
| **Server outside the peers** | Any machine, other than the session's peers and the NAT devices on their path, that a connectivity route needs to be running (§3.4). |
| **Lockstep**, **state synchronization**, **rollback** | The replication families of §3.1. |

---

## 3. Core model

### 3.1 Replication families

The choice is decision Q1; §5 is written for the recommended family and says
where the alternative changes it.

| Family | What travels | What it demands of the engine | Source |
|---|---|---|---|
| **Deterministic lockstep** | Inputs only | Every peer runs the whole simulation and gets a bit-identical result; a peer advances tick *n* only once it has every input for *n*, so the session runs at the slowest link's pace | Glenn Fiedler, [Deterministic Lockstep](https://gafferongames.com/post/deterministic_lockstep/), 2014-11-29 |
| **State synchronization** | Inputs and state | The authority sends state for a prioritised subset of objects within a bandwidth budget (a priority accumulator); "Because we send state, we don't need perfect determinism to stay in sync" | Glenn Fiedler, [State Synchronization](https://gafferongames.com/post/state_synchronization/), 2015-01-05 |
| **Rollback** | Inputs, predicting missing remote ones | Lockstep's determinism plus saving, loading and re-running a frame without rendering it, every frame; random generators deterministic and part of the saved state | [GGPO](https://www.ggpo.net/); [GGPO Developer Guide](https://github.com/pond3r/ggpo/blob/master/doc/DeveloperGuide.md) |

The ROADMAP row says "deterministic ECS state replication". This document
reads that as a requirement on the *replication* (the same checkpoint and the
same message stream produce the same guest view), not as bit-identical
simulation on every peer. §3.2 is why; Q1 asks the owner to confirm.

### 3.2 Determinism inventory

Each source of divergence between two runs of one session, with its
disposition per family, at HEAD on 2026-11-01. Phase 0 replaces this
hand-written Today column with the determinism section of the replication
design, kept current by a test (COOP-DET-006).

| # | Source | Today | State sync (recommended) | Lockstep |
|---|---|---|---|---|
| 1 | Gameplay delta time | `about_to_wait` (`byroredux/src/app_events.rs:1401-1423`) feeds the frame's wall-clock `wall_dt` into `DeltaTime`; `BYROREDUX_FIXED_DT` and `--bench-mode` override it for diagnostics and benches only | Host only; tolerated | Must become a fixed tick |
| 2 | Physics substep count | Fixed `PHYSICS_DT` with an accumulator, but `PhysicsWorld::step` (`crates/physics/src/world/mod.rs:1373`) stops catching up once `SUBSTEP_TIME_BUDGET` of wall time is spent (`:53`), and the static-scene fast path zeroes the accumulator (`:1446`) | Host only; tolerated | Must be decoupled from wall time |
| 3 | Rapier determinism level | `rapier3d = "0.36"` with no features (`Cargo.toml:186`; lock: 0.36.1). The guide calls a default build "locally deterministic"; cross-platform determinism needs the `enhanced-determinism` feature, IEEE 754-2008 targets and identical insertion order, and that feature "cannot be enabled at the same time as the `simd8` feature" ([Rapier: Determinism](https://rapier.rs/docs/user_guides/rust/determinism)) | Not required | Required; **Phase 0: verify** that the guide applies to 0.36.1 and whether the always-on SIMD noted at `Cargo.toml:185` is the `simd8` lane width |
| 4 | Parallel scheduler | A stage's parallel batch runs on rayon (`Scheduler::run`, `crates/core/src/ecs/scheduler.rs:475`); order inside a batch is unspecified. `boot::schedule::scheduler_access_report_tests::build_scheduler_reports_zero_access_conflicts` (`byroredux/src/boot/schedule/mod.rs:578`) asserts no declared write conflict inside a batch | Host only; tolerated | Order-independent only if every declaration is honest (COOP-DET-004) |
| 5 | Collider registration order | Shape conversion on rayon; insertion stays serial "in newcomer order" (`register_newcomers`, `crates/physics/src/sync.rs:1005-1033`) | Host only | Newcomer order must itself be deterministic (row 6) |
| 6 | Entity id and spawn order | Ids are a monotonic `u32` (`World::spawn`, `crates/core/src/ecs/world.rs:110`). Exterior cells are pre-parsed on a worker and drained with `try_recv` (`byroredux/src/streaming/pre_parse.rs:218`), so spawn order can depend on worker timing. **Phase 0: verify** whether it does | Ids are host-local; the wire uses stable identity (COOP-REP-003) | Must be deterministic |
| 7 | Script and dialogue random draws | `StoryManagerRng` and `DialogueRandomState` are seeded from `SystemTime` (`crates/scripting/src/story_manager.rs:310`, `crates/scripting/src/dialogue.rs:69`); `Utility.RandomInt` / `RandomFloat` and `GetRandomPercent` draw from `StoryManagerRng`. Both are `NOT_SAVED_BY_DESIGN` (`byroredux/src/save_io/registry_completeness_tests.rs:592-594`) | Host only; draws arrive as results | Must be seeded per session and saved |
| 8 | Presentation random draws | Particles use an xorshift seeded per entity from `TotalTime` in milliseconds (`particle_system`, `byroredux/src/systems/particle.rs:399-400`; its doc comment still says frame count); wander timing hashes the FormId (`byroredux/src/systems/wander.rs:112`) | Particles local (not replicated); wander host only | Deterministic given rows 1, 6 and 10 |
| 9 | Papyrus VM budget | Counted in statements, not clock time (`VmFrameBudget`, `crates/scripting/src/fragment/budget.rs:59`), renewed per frame | Host only | Deterministic per tick once row 1 is fixed |
| 10 | Script reads of "real" time | `Utility.GetCurrentRealTime` returns `TotalTime` (`crates/scripting/src/fragment/vm/natives/game_utility.rs:154-156`), the sum of each frame's gameplay delta (`app_events.rs:1460-1461`), which stops while the pause surface is open. The CK defines it as real-world seconds since launch, with menu time counted (CK wiki dump, `ck-uesp-wiki/(main)/GetCurrentRealTime - Utility.wiki`) | Host only | Deterministic once row 1 is fixed; the CK's wall-clock meaning cannot be kept |
| 11 | Hash-container iteration | Simulation crates use `std` `HashMap` / `HashSet`, whose iteration order varies per process; the `Snapshot` doc says nested map order "can vary between runs" (`crates/save/src/snapshot.rs:620`). `rg -l 'std::collections::(HashMap\|HashSet)' crates/scripting byroredux/src` lists the files | Matters only for the state hash (COOP-DET-002) | Every iteration feeding simulation must be ordered |
| 12 | Time freezes | Loading pins delta time to zero. VATS publishes 0.0 (`VatsState::world_time_scale`, `byroredux/src/components/vats.rs:82`, #6350). `DebugUiState::simulation_paused` (`crates/debug-ui/src/lib.rs:348`) stops `Scheduler::run` for the pause, settings and inventory pages, not for dialogue, and the frame keeps only the debug drain (`run_exclusive_named`, `app_events.rs:1499`) | Session policy (§5.7) | Session policy (§5.7) |
| 13 | Floating point across machines and builds | No simulation-level determinism test exists | Not required | Required across every peer's CPU and build |

Rows 1, 2, 3, 4, 6, 7, 11 and 13 stand between HEAD and lockstep. State sync
needs none of them fixed, and rows 7 and 11 only for its diagnostics.

### 3.3 Session roles

| State | Host | Guest | How a guest is told |
|---|---|---|---|
| Simulation (AI, physics, scripts, quests, combat resolution) | Runs it | Receives results | Replicated state |
| Own character movement | Runs it authoritatively | Predicts, corrected by the host | Correction messages |
| Rendering, audio, UI, camera | Local | Local | n/a |
| Saves | Writes the session save | Receives its character export (§10 Q4) | Notification |
| State-mutating console and debug commands | Allowed | Refused | Console line naming the host |
| Pause, wait, VATS | Per §5.7 | Per §5.7 | HUD notice |

### 3.4 Connectivity routes

The anti-scope forbids the engine shipping or defaulting to a server. This
table states, for every route §10 Q2 weighs, whether it needs a server
outside the peers and who would run it.

| Route | Server outside the peers | What the engine contacts | Source |
|---|---|---|---|
| **Direct address** (LAN, or a host port already reachable) | None | Only the address the user enters | — |
| **PCP / NAT-PMP port mapping** | None. The request goes to the NAT on the path (the user's router, or a carrier NAT that implements the protocol) | The NAT gateway | [RFC 6887](https://www.rfc-editor.org/rfc/rfc6887.html) (2013), [RFC 6886](https://www.rfc-editor.org/rfc/rfc6886.html) (2013) |
| **User-run overlay network** (Skyrim Together documents ZeroTier, [server guide](https://wiki.tiltedphoques.com/tilted-online/guides/server-guide/overview.md)) | Yes, outside the engine. ZeroTier clients use "a default pool of global root servers deployed and maintained by ZeroTier, Inc."; user-defined roots exist but "are **no longer recommended**" ([ZeroTier: Roots](https://docs.zerotier.com/roots)) | Only the overlay addresses the user enters; the overlay client is the user's own install | as cited |
| **ICE with STUN / TURN** | Yes: whoever runs the STUN and TURN servers. ICE "is typically used in concert with STUN or TURN servers in the network" ([RFC 8445](https://www.rfc-editor.org/rfc/rfc8445.html) §2, 2018); hole punching needs a rendezvous both peers reach (Ford, Srisuresh, Kegel, *Peer-to-Peer Communication Across Network Address Translators*, USENIX ATC 2005, <https://bford.info/pub/net/p2pnat/>) | The servers the user configures | as cited |
| **GameNetworkingSockets** (BSD-3-Clause) | Yes: a signaling service ("Plug in your own signaling service"); its relay network SDR is Steam-only, "at this time we are not able to offer access to SDR on other platforms to all partners" ([README](https://github.com/ValveSoftware/GameNetworkingSockets)) | The signaling service, plus Valve's relays on Steam | as cited |
| **LAN only** | None | Peers on the local network | — |

---

## 4. Goals and non-goals

### 4.1 Goals

- **COOP-G-001 — Play together.** Two to four peers on one Wave 1 title
  complete that title's playable-slice routes (door, traversal, melee and
  loot, the authored quest objective) in one session, each guest's
  character taking part and each peer's view agreeing with the host (§7.2).
- **COOP-G-002 — No server.** A session forms with no server the project
  operates and none the engine contacts by default (§3.4). With no session
  open, the engine opens no non-loopback socket.
- **COOP-G-003 — Join late.** A guest joins a session in progress and plays
  within a bounded time, published per title.
- **COOP-G-004 — Survive the host.** If the host leaves, the session either
  continues under a new host or ends with every peer holding a recoverable
  save (§10 Q6); nobody loses more than a bounded window of play.
- **COOP-G-005 — Honest mechanics.** Every original mechanic that assumes
  one player has a stated co-op behaviour, reported in game when it applies
  (§5.7, §6).
- **COOP-G-006 — Measured.** Bandwidth, correction distance, join time and
  migration loss are published per title, with one home (§8).

### 4.2 Non-goals

- **COOP-NG-001.** Any engine-operated or engine-defaulted server:
  matchmaking, lobby lists, master servers, rendezvous, relays, accounts
  ([ROADMAP anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
- **COOP-NG-002.** Telemetry or crash reports about sessions (same
  anti-scope).
- **COOP-NG-003.** Sending game data, mod files or archives between peers.
  Each peer brings its own installed copy; the manifest check compares
  hashes, never bytes (COOP-SES-004).
- **COOP-NG-004.** More than four peers; the ROADMAP row caps the milestone.
- **COOP-NG-005.** Competitive play, anti-cheat, and protection against a
  hostile peer beyond refusing malformed messages (COOP-NET-005).
- **COOP-NG-006.** Voice chat.
- **COOP-NG-007.** Interoperating with Skyrim Together, NV:MP or any other
  community multiplayer mod, or with the original executables.
- **COOP-NG-008.** Peers on different engine builds in v1 (§10 Q9).
- **COOP-NG-009.** Fallout 76 (§7.1).
- **COOP-NG-010.** A route that needs a server outside the peers (§3.4) as a
  required or default path (§10 Q2).

---

## 5. Requirements by area

Every "Today" below was checked at HEAD on 2026-11-01.

### 5.1 Session (`COOP-SES`)

- **COOP-SES-001.** A fifth peer's join MUST be refused with a reason the
  joining peer displays. A session has exactly one host (§2).
- **COOP-SES-002.** Hosting and joining MUST be reachable from the CLI and
  from the launcher's boot contract
  ([BootRequest](../engine/launcher.md#2-the-bootrequest-contract)); joining
  takes an address the user supplies. Today neither entry point exists (no
  `coop`, `--host` or `--join` in `byroredux/src/boot/` or
  `crates/boot-request/src/`).
- **COOP-SES-003.** A join without the host-generated session secret
  (passed out of band) MUST be refused before any world state is sent.
- **COOP-SES-004.** A join whose content manifest differs from the host's
  MUST be refused before any world state is sent, naming every differing
  entry on both peers. Filename identity is not enough: today a plugin's
  identity is a UUIDv5 of its filename (`PluginId::from_filename`,
  `crates/core/src/form_id.rs:98-100`), so two builds of a same-named plugin
  share Form IDs.
- **COOP-SES-005.** A join from a different engine build or wire protocol
  version MUST be refused with both versions shown (COOP-NG-008).
- **COOP-SES-006.** A guest leaving MUST remove (or park, per §10 Q4) its
  guest character without stopping the session for the remaining peers.
- **COOP-SES-007.** `coop.status` (new console command, reachable through
  `byro-dbg`) MUST print the session id, role, peer list with round-trip
  time, and the manifest hash.

### 5.2 Transport and connectivity (`COOP-NET`)

- **COOP-NET-001.** With no session open, the engine MUST open no socket
  other than the loopback debug server, pinned by a test (§7.2 A3). Today the
  only listener binds `127.0.0.1` (`crates/debug-server/src/listener.rs:173`),
  release builds start it only with `BYRO_DEBUG_SERVER=1`
  (`byroredux/src/main.rs:1198-1212`), and the Scaleform navigator rejects
  non-local URLs (`crates/ui/src/navigator.rs:943`).
- **COOP-NET-002.** A guest MUST be able to join by the host's IP address and
  port, on a LAN or wherever the host's port is reachable.
- **COOP-NET-003.** On the user's opt-in, the host SHOULD request an inbound
  port mapping through PCP or NAT-PMP (§3.4) and MUST report in
  `coop.status` whether the mapping succeeded.
- **COOP-NET-004.** The engine MUST accept a user-run overlay network's
  addresses like any other and MUST NOT bundle, configure or contact such a
  network itself (§3.4: its servers are outside the engine).
- **COOP-NET-005.** A message that fails authentication or decoding MUST be
  dropped, counted in `coop.status`, and never applied. All session traffic
  is encrypted and authenticated with keys derived from the session secret;
  **Phase 0: verify** which established protocol meets this (the
  replication design picks and cites one).
- **COOP-NET-006.** The wire protocol MUST run on an unreliable datagram
  transport and keep A5's conditions under emulated loss, duplication and
  reordering (§7.2 A6).

### 5.3 Determinism and the state hash (`COOP-DET`)

- **COOP-DET-001.** With a session open, `StoryManagerRng` and
  `DialogueRandomState` MUST be seeded from the session and saved in the
  session save, so a resumed session draws the same sequence. Today both are
  time-seeded and unsaved (§3.2 row 7).
- **COOP-DET-002.** Host and guest MUST compute the same canonical state
  hash over the replicated state, independent of hash-container iteration
  order (§3.2 row 11). Today the snapshot payload is not canonical
  (`crates/save/src/snapshot.rs:620`).
- **COOP-DET-003.** `coop.hash` (new console command) MUST print the current
  tick and the state hash.
- **COOP-DET-004.** `build_scheduler_reports_zero_access_conflicts` MUST stay
  green with every system the replication layer adds declaring its access.
- **COOP-DET-005.** If Q1 chooses lockstep, a two-run replay test over a
  recorded input stream MUST produce equal state hashes, and every §3.2 row
  marked "Must" MUST be closed by a named test before Phase 3 starts. Under
  state sync this does not apply.
- **COOP-DET-006.** A test MUST fail when a `SystemTime` or `Instant` read
  appears in a simulation crate without a row in the replication design's
  determinism table.
- **COOP-DET-007.** A periodic hash exchange MUST report a desync with the
  tick and the first differing component column.

### 5.4 Replication (`COOP-REP`)

- **COOP-REP-001.** A guest MUST NOT write simulation state except through
  its input frames; the host is the authority for the rest (§3.3).
- **COOP-REP-002.** Replicated state MUST be declared in a registry, one row
  per component or resource, the way `SaveRegistry` declares saved state
  ([Save/Load § 2](../engine/save-load-roundtrip.md#2-ecs-snapshot-capture)),
  with a completeness test that classifies every saved type as replicated or
  host-only with a reason, mirroring `registry_completeness_tests.rs`.
- **COOP-REP-003.** A raw `EntityId` MUST NOT appear in a message. Entities
  are named by the `FormIdPair` for placed references
  ([stream-boundary §3](../engine/stream-boundary-state-continuity.md#3-formid-keyed-state-not-entityid-keyed-state))
  and a host-issued network id for runtime spawns. Today `EntityId` is a
  session-local `u32` ([ECS § Entities](../engine/ecs.md#entities)).
- **COOP-REP-004.** The host MUST send each guest only its relevance set,
  prioritised by a priority accumulator within a per-peer bandwidth budget
  (§3.1, Fiedler).
- **COOP-REP-005.** A guest MUST predict its own character's movement from
  its own input frames and reconcile against the host's correction; the
  correction distance is a §8 metric.
- **COOP-REP-006.** A guest MUST show other actors interpolated between
  received states and MUST NOT run their AI.
- **COOP-REP-007.** Presentation-only systems (particles, audio, camera, UI
  animation, render-side state) MUST run locally on every peer and MUST NOT
  be replicated.
- **COOP-REP-008.** The replication design MUST list which scheduler systems
  run on a guest; under the recommended model none of the simulation stages'
  systems runs over replicated entities.
- **COOP-REP-009.** The per-peer bandwidth budget (COOP-REP-004) MUST be a
  setting.

### 5.5 Players in one world (`COOP-WLD`)

- **COOP-WLD-001.** The host's world MUST hold one player character per
  peer. Today the player is a singleton resource, `PlayerEntity`
  (`byroredux/src/systems/character.rs:44`), read across the binary; the
  replication design states which reads mean "the local player", "the
  host's player" or "any player".
- **COOP-WLD-002.** Each peer's camera, input, HUD and audio listener MUST
  follow that peer's own character.
- **COOP-WLD-003.** Every player's character MUST stay inside a loaded cell,
  by the rule §10 Q3 picks. Today `compute_streaming_deltas`
  (`byroredux/src/streaming/mod.rs:897`) takes one `player_grid`, and
  `CurrentCellRoot` (`byroredux/src/cell_loader/transition.rs:135`) holds
  one interior root.
- **COOP-WLD-004.** A cell transition (door, fast travel, script teleport)
  by any player MUST follow §10 Q3's party rule and MUST name the triggering
  peer to every peer.
- **COOP-WLD-005.** Combat MUST treat every player character as a valid
  target, ally and aggressor. Today combat, crime and faction-hostility code
  read `PlayerEntity` (`byroredux/src/combat.rs`, `byroredux/src/crime.rs`,
  `byroredux/src/systems/faction_hostility.rs`); the replication design lists
  every system this touches.

### 5.6 Scripts and quests (`COOP-SCR`)

- **COOP-SCR-001.** Scripts (Papyrus VM, ObScript runtime, effect lowering)
  MUST run only on the host; their effects reach guests as replicated state.
  Today every interpreter system is an exclusive `Update` system under one
  per-frame statement allowance
  ([Scripting § frame budget](../engine/scripting.md#the-frame-budget-m472-5763)).
- **COOP-SCR-002.** `Game.GetPlayer()`, the `Player` reference in
  conditions, and ObScript's `player` MUST resolve per §10 Q10; event
  arguments that name an actor (`akActionRef`, the activator of an
  `OnActivate`) MUST name the acting peer's character.
- **COOP-SCR-003.** Quest stages, objectives and journal state MUST be shared
  by the session and shown in every peer's HUD.
- **COOP-SCR-004.** A peer MUST NOT be able to choose topics in a
  conversation another peer started.
- **COOP-SCR-005.** Sandboxed extensions MUST run only on the host, their
  package set is part of the content manifest (COOP-SES-004), and a grant
  MUST be identical on every peer. Their clock and random interfaces are
  absent unless granted
  ([SEC-002](../engine/sandboxed-linked-mods.md#102-requirements); pinned by
  `wasi_imports_are_absent_by_default`, `crates/mod-runtime/src/tests.rs:2434`).
- **COOP-SCR-006.** State-mutating console commands MUST be refused on a
  guest with a message naming the host.
- **COOP-SCR-007.** Peers within hearing range of a conversation they do not
  own SHOULD see its subtitles.

### 5.7 Time, pause and single-player mechanics (`COOP-TIME`)

- **COOP-TIME-001.** While a guest is connected, opening any menu MUST NOT
  stop the host's simulation. Today the pause, settings and inventory pages
  stop `Scheduler::run` (§3.2 row 12).
- **COOP-TIME-002.** A request to change the session time scale MUST follow
  §10 Q5, stated against the canonical `world_time_scale` value, not per
  game. Today VATS publishes 0.0 on Fallout 3 and New Vegas; Fallout 4's
  slow-motion lane is owned by the [VATS PRD](vats.md) (VATS-FO4,
  VATS-PLAY-005).
- **COOP-TIME-003.** A refused wait, sleep or fast travel (§10 Q5's party
  rule) MUST tell the requesting peer who blocked it.
- **COOP-TIME-004.** A loading screen on one peer MUST NOT freeze the others;
  a guest still loading appears as not yet present.
- **COOP-TIME-005.** A peer in a menu MUST be shown to the other peers as
  busy.

### 5.8 Join, saves and migration (`COOP-SAV`)

- **COOP-SAV-001.** Late join MUST deliver the host's state as a checkpoint
  (the M45 snapshot plus the content manifest it was taken under), applied
  through live load-apply
  ([Save/Load § 6](../engine/save-load-roundtrip.md#6-live-load-apply-m451))
  before the guest switches to the replication stream. Today a snapshot
  carries no load order: `ActiveLoadOrder` and `LoadOrderIdentity` are
  `NOT_SAVED_BY_DESIGN` (`registry_completeness_tests.rs:560`, `:1031`).
- **COOP-SAV-002.** A guest MUST refuse a checkpoint that fails the save
  validation gates
  ([Save/Load § 3](../engine/save-load-roundtrip.md#3-validation-gates)),
  naming the failing gate.
- **COOP-SAV-003.** Only the host MUST write session saves, and a session
  save MUST record every guest character (§10 Q4) and the session RNG state
  (COOP-DET-001).
- **COOP-SAV-004.** A session save opened without a session MUST load as the
  host character's single-player save, guest characters parked.
- **COOP-SAV-005.** When the host leaves, every remaining peer MUST hold a
  save no older than the checkpoint interval (a setting), whichever way §10
  Q6 is decided.
- **COOP-SAV-006.** If migration is chosen, every peer MUST compute the new
  host from the peer list by the same rule, with no vote round, and
  `coop.status` MUST print the rule.

### 5.9 Diagnostics (`COOP-DX`)

- **COOP-DX-001.** `coop.net` (new console command) MUST print per-peer send
  and receive bandwidth, loss, round-trip time and the budget's fill.
- **COOP-DX-002.** Every refusal (join, manifest, version, console, time
  scale) MUST reach the affected peer as a notification through the
  existing `PlayerNotifications` path (`byroredux/src/notifications.rs:14`).
- **COOP-DX-003.** A session MUST be able to record its input and message
  stream to a local file for offline replay against the host's checkpoint;
  nothing leaves the machine (COOP-NG-002).

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Single-player; co-op only as a community retrofit (§1) | Up to four players, host-authoritative (COOP-REP-001) | Retrofit mods' engine hooks | Content authored for one player behaves oddly with several → rows 2–6 |
| 2 | One `Player` reference every script and condition means | Resolved per §10 Q10 (COOP-SCR-002) | — | Quests that test "is the player" fail to credit a guest; the replication design's script survey reports them |
| 3 | Menus pause the world (HEAD: §3.2 row 12). Skyrim Together already disables pausing for several menus (§1) | No pause while a guest is connected (COOP-TIME-001) | — | A peer in a menu can be attacked; shown busy (COOP-TIME-005), Q5 |
| 4 | VATS freezes (FO3, FNV) or slows (FO4) the world ([VATS PRD](vats.md)) | Per §10 Q5 (COOP-TIME-002) | — | Balance changes whichever way Q5 goes; the in-game notice states the rule |
| 5 | Wait, sleep and fast travel advance or skip time for the one player | Party rule (COOP-TIME-003) | — | One peer can block another; the refusal names who |
| 6 | A levelled-list request carries "a desired level, usually the player's level, possibly modified by the level of the encounter zone" (UESP, local dump `uesp-wiki/Skyrim/Leveled Lists.wiki`; other titles **Phase 0: verify**); the engine's counterpart is `CalculateEncounterLevel` ([Scripting](../engine/scripting.md#calculateencounterlevel-m472)) | Scales to whichever character §10 Q10 names as the player | — | A low-level guest faces the host's level. Tilted Online's feature list covers XP sync but not level scaling; **Phase 0: verify** what Skyrim Together and NV:MP do, from their own documentation or source |
| 7 | Saves at any time by the one player | Host-only session saves (COOP-SAV-003) | — | A guest cannot save alone; it receives its character export (Q4) |
| 8 | Skyrim Together hosts through a game server DLL run by a dedicated-server executable (README, §1) | The host is an ordinary engine peer | Dedicated-server installs | The host's frame time carries every guest's simulation; measured (§8 host cost) |
| 9 | Skyrim Together hosts reach peers by port forwarding or ZeroTier (server guide, §3.4) | Same routes plus opt-in PCP / NAT-PMP (COOP-NET-002..004) | — | A host behind a NAT it cannot map needs an overlay whose servers are a third party's; Q2 |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail).
A title enters a wave only once its single-player playable-slice routes pass,
since co-op acceptance replays them with more players. No requirement here
branches per game: the per-game mechanics (VATS, script dialect) reach the
replication layer as canonical values such as `world_time_scale` and through
the scripting runtime's own translate step.

| Game | In scope | Why / wave |
|---|---|---|
| Fallout: New Vegas | yes | Wave 1. Tier 1, and the second playable-slice route (`fixtures/fnv.env` declares no gate restriction; `p4-fnv-quest-route.sh`, `p5-fnv-quest-persistence.sh`). Carries the VATS case (Q5) and the ObScript runtime |
| Skyrim (LE / SE / AE) | yes | Wave 1. Tier 1, the first playable-slice route, the default title of every fixture-driven smoke script, the Papyrus VM, and Skyrim Together's target (SE). LE counts with SE, as in the [LMC waves](legacy-mod-compatibility.md#71-waves) |
| Fallout 3 | yes | Wave 2. Shares New Vegas's machinery; its fixture declares only part of the route set (`fixtures/fo3.env:9`) |
| Fallout 4 | yes | Wave 2. Tier 1; VATS slow motion (Q5) and workshop state widen the replicated set |
| Starfield | yes | Wave 3, once it reaches Tier 1 (Tier 2 at HEAD) |
| Oblivion | yes | Wave 3, once its playable slice passes (Tier 3 at HEAD) |
| Fallout 76 | no | Tier 3 with no playable single-player slice to share; re-evaluated if that changes (COOP-NG-009) |

### 7.2 Acceptance

Scenarios run on one machine with two to four engine instances on loopback,
each with its own `BYROREDUX_SETTINGS_PATH` and `BYROREDUX_SAVE_DIR`, over the
title's installed data; no game or mod file is checked in. Impairment uses
`tc netem` on the loopback interface
([tc-netem(8)](https://man7.org/linux/man-pages/man8/tc-netem.8.html)); a
script that cannot configure it exits 77 (SKIP), never a pass. **Phase 0:
verify** that four Vulkan instances fit the 12 GB development GPU; if they do
not, A4–A9 run guests on a second machine and say so in their output.

| # | Scenario | Driven by | Data | Passes when |
|---|---|---|---|---|
| A1 | A host `World` replicates to a guest mirror in one process over an in-memory channel | new `cargo test -p byroredux --bin byroredux coop::mirror_tests` | synthetic | State hashes (COOP-DET-002) equal after every tick of a scripted run |
| A2 | Manifest refusal | new `cargo test` over two manifests differing in one plugin hash | synthetic | The join is refused and both entries are named (COOP-SES-004) |
| A3 | No socket beyond the debug server | new source-scan `cargo test` (the repository's text-scan guard pattern) | none | Every `bind` / `connect` / `UdpSocket` site in the workspace is the debug server or the session transport module (COOP-NET-001) |
| A4 | Loopback join: two peers, then four, then a refused fifth | new `docs/smoke-tests/coop1-loopback-join.sh <game>`, `--bench-hold` + `byro-dbg` `coop.status` on each peer | FNV, Skyrim SE | Peer lists agree; the fifth join is refused (COOP-SES-001) |
| A5 | Playable-slice routes with a guest present: the host runs `p0-door-interaction` and `p2-melee-core` while a guest follows, then the quest route (`p4-fnv-quest-route.sh` / `p4-quest-route.sh`) | new `coop2-slice-routes.sh <game>`, reusing `docs/smoke-tests/fixtures/` | FNV, Skyrim SE | Each route's existing pass condition holds on the host, `coop.hash` agrees across peers at each route checkpoint, and the guest's HUD shows the objective |
| A6 | A5 under 100 ms delay and 2 % loss | A5 with `tc netem` | FNV, Skyrim SE | A5's conditions hold; correction distance within the Phase 4 target |
| A7 | Late join mid-route | new `coop3-late-join.sh <game>` | FNV, Skyrim SE | The guest is controllable within the published join time; hashes agree after join |
| A8 | Session save, then solo reload | `coop3-late-join.sh`, then a reload of the host save without a session in the style of the title's P5 persistence script (`p5-fnv-quest-persistence.sh` / `p5-quest-persistence.sh`) | FNV, Skyrim SE | Loads as the host's single-player save with guest characters parked (COOP-SAV-004) |
| A9 | Host loss | new `coop4-host-loss.sh <game>`: kill the host process mid-route | FNV, Skyrim SE | Per Q6: the session continues under the elected host, or every peer holds a loadable save no older than the checkpoint interval (COOP-SAV-005) |

A title passes a wave when A4–A9 pass for it and A1–A3 are green in CI.

---

## 8. Metrics

The numbers live in ROADMAP, in a co-op table that Phase 1 adds under the
M60 row (one row per title, one column per metric below). The
[compatibility matrix](../../ROADMAP.md#compatibility-matrix) stays the
home of parse rates and gains only a link to it.

| Metric | Definition | Granularity |
|---|---|---|
| Route pass rate | §7.2 scenarios passing ÷ scenarios applicable | per title, per wave |
| Bandwidth | mean and 95th-percentile bytes per second from the host to one guest during A5 | per title, per peer count |
| Correction distance | 95th-percentile distance between a guest's predicted own position and the host's correction | per title, with and without A6 impairment |
| Desync count | hash mismatches (COOP-DET-007) per hour of A5 play | per title |
| Join time | seconds from join request to the guest's first controllable frame (A7) | per title |
| Migration loss | simulated seconds between the last checkpoint and host loss, plus resume time (A9) | per title |
| Host cost | host frame time with three guests ÷ host frame time solo, same route and pinned camera, per the ROADMAP bench methodology | per title |
| Determinism inventory | open §3.2 rows ÷ rows, as kept by COOP-DET-006 | per release |

---

## 9. Delivery sequence

Tier 9 carries "no active work until M40 (streaming) and M45 (save/load)
prove the underlying state model holds up"
([ROADMAP Tier 9](../../ROADMAP.md#tier-9--better-than-bethesda-capability-stretch-post-tier-4-horizon)).
Both are under Completed Milestones; whether they *prove* the model is the
owner's call, made by approving this PRD and scheduling Phase 0.

**Phase 0 — Determinism analysis, replication design (no engine code).**
Write `docs/engine/coop-replication.md`: the replication registry's
classification of every saved type, the wire protocol, the encryption and
authentication protocol chosen with its citation (COOP-NET-005), the
scheduler systems that run on a guest (COOP-REP-008), the `PlayerEntity` read
survey (COOP-WLD-001, COOP-WLD-005), the script `Player` survey
(COOP-SCR-002), and the determinism section that closes §3.2 (§10 Q8). Close
the **Phase 0: verify** items: Rapier's guide against 0.36.1 and its SIMD
build (§3.2 row 3), streaming spawn order (row 6), levelled-list requests on
the other titles and encounter scaling in Skyrim Together and NV:MP (§6 row
6), NV:MP's architecture and its handling of VATS from its own documentation
or source (Q5), and the four-instance VRAM footprint (§7.2). Exit: the design
is checked in and reviewed, and every §3.2 row has a disposition.

**Phase 1 — Session RNG, state hash, in-process mirror.** COOP-DET-001..003,
COOP-DET-006, COOP-REP-002, COOP-REP-003, the in-memory host→guest mirror and
the ROADMAP co-op table (§8). Exit: §7.2 A1 and A2 green in CI.

**Phase 2 — Transport and session.** COOP-SES-001..007, COOP-NET-001..006,
the launcher and CLI entry points. Exit: A3 green in CI; A4 passes on FNV and
Skyrim SE.

**Phase 3 — Players in one world.** COOP-WLD-001..005, COOP-REP-001,
COOP-REP-004..009, COOP-TIME-001..005, COOP-DET-007. Exit: A5's door and
traversal routes pass with one guest on both Wave 1 titles; bandwidth and
correction distance published.

**Phase 4 — Gameplay and quests.** COOP-SCR-001..007, combat against several
players, the full A5 and A6. Exit: A5 and A6 pass on both Wave 1 titles;
desync count and host cost published.

**Phase 5 — Late join and session saves.** COOP-SAV-001..004. Exit: A7 and A8
pass on both Wave 1 titles; join time published.

**Phase 6 — Host loss.** COOP-SAV-005, COOP-SAV-006, COOP-DX-003. Exit: A9
passes on both Wave 1 titles; migration loss published.

**Phase 7 — Later waves.** Fallout 3 and Fallout 4, then Starfield and
Oblivion as their tiers allow. Exit: A4–A9 pass per title; route pass rate
published per wave.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — Replication model.** Recommended: host-authoritative state
   synchronization (§3.1). The host simulates; guests send inputs, receive
   prioritised state and predict only their own character. Consequence:
   none of the §3.2 lockstep rows has to close, peers may run at different
   frame rates, determinism is a diagnostic (the state hash), the ROADMAP
   row's "deterministic ECS state replication" is read as §3.1 states, the
   host carries every guest's simulation cost (§8 host cost), and host loss
   needs checkpoints (Q6). Alternative A: deterministic lockstep.
   Consequence: inputs-only bandwidth and full state on every peer (trivial
   migration), but every "Must" row of §3.2 closes first, the session runs
   at the slowest peer's pace (Fiedler, *Deterministic Lockstep*), and late
   join still needs a checkpoint. Alternative B: rollback (GGPO).
   Consequence: everything A needs, plus a per-frame save, load and
   re-simulate; the M45 snapshot's load path reloads the cell
   ([Save/Load § 6](../engine/save-load-roundtrip.md#6-live-load-apply-m451)),
   so this needs a second state model.
2. **Q2 — Connectivity without a server.** Each option's server need is in
   §3.4. Recommended: direct address, opt-in PCP / NAT-PMP, and accepting
   user-run overlay addresses (COOP-NET-002..004). Needs a server outside
   the peers: no, for the engine. The engine contacts only addresses the
   user enters and the NAT on its own path. An overlay the user chooses
   (ZeroTier by default uses ZeroTier, Inc.'s roots) is a third party's
   server, but the engine neither bundles, defaults to nor contacts it,
   which the anti-scope allows. Consequence: a host behind a NAT it cannot
   map needs that overlay. Alternative A: ICE with STUN / TURN servers the
   user configures, empty by default. Needs a server outside the peers:
   yes, whoever runs STUN / TURN. Consequence: more NATs traversed; any
   default server would breach the anti-scope. Alternative B: Valve's
   GameNetworkingSockets. Needs a server outside the peers: yes, a
   signaling service, and Valve's Steam-only relays on the reliable path.
   Consequence: against the anti-scope (COOP-NG-010). Alternative C: LAN
   only. Needs a server outside the peers: no. Consequence: the simplest
   promise, and it excludes friends in other homes.
3. **Q3 — Where players can be.** Recommended for v1: one location context
   per session (one interior, or one exterior worldspace). Guests are
   tethered to the host's streamed radius; a door or fast travel moves the
   whole party once the other peers confirm. Consequence: single-centre
   streaming and the single `CurrentCellRoot` stay as they are
   (COOP-WLD-003). Alternative A: one context, streaming the union of every
   player's radius. Consequence: multi-centre `compute_streaming_deltas` and
   a higher host cost. Alternative B: players anywhere, the host simulating
   several contexts. Consequence: the one-context world model is refactored
   throughout; the largest option.
4. **Q4 — Whose characters, whose save.** Recommended: the session is the
   host's world and save. Each guest character is stored in the session save
   under a guest key; on leaving, the guest receives a character export
   (actor values, inventory, appearance) for a later session, and its own
   single-player saves are untouched. Consequence: a guest's quest progress
   belongs to the host's world. Alternative: guest characters are created
   fresh per session and nothing is kept. Consequence: simpler, no guest
   persistence.
5. **Q5 — Time scale, wait and pause.** Recommended: no peer can stop or slow
   the shared clock while another is connected. VATS (Fallout 3, New Vegas,
   and Fallout 4's slow motion, as specified in the [VATS PRD](vats.md)) is
   unavailable in a session, with a notice; wait, sleep and fast travel need
   every peer to confirm. Consequence: VATS players lose the mechanic in
   co-op, and the VATS PRD gains a session gate. What NV:MP does with VATS
   is **Phase 0: verify** from its own documentation or source.
   Alternative A: a time-scale request pauses or slows the whole session.
   Consequence: any peer can freeze the others. Alternative B: VATS runs in
   real time for its user. Consequence: the mechanic's balance changes and
   needs its own design, in the VATS PRD.
6. **Q6 — Host loss.** Recommended: Phase 6 ships end-with-saves (every peer
   keeps the last checkpoint as a save and the session ends); migration
   follows, the elected peer applying the last checkpoint through live
   load-apply and the others rejoining. Consequence: the ROADMAP row's "host
   migration" lands after v1; loss is bounded by the checkpoint interval
   either way. Alternative: migration in Phase 6. Consequence: Phase 6 grows
   by the election rule, a resume through a live cell reload, and rejoin,
   before any title has shipped co-op.
7. **Q7 — First wave.** Recommended: New Vegas and Skyrim SE (§7.1), one
   title per script runtime. Alternative: Skyrim SE alone. Consequence: the
   best-documented precedent first, and the VATS question (Q5) deferred.
8. **Q8 — The R5 precondition.** The ROADMAP row says "R5's quest prototype
   must include a determinism analysis before this can ship". R5 closed on
   2026-05-16
   ([roadmap-history §6](../archive/roadmap-history.md#6-risk-reducer-index-growth-tripwire-and-pacing-discipline)),
   and its evaluation has no determinism section
   ([r5-evaluation.md](../r5-evaluation.md#verdict)). Recommended: this
   PRD's Phase 0 determinism section replaces the precondition, and the
   ROADMAP row is edited to say so. Consequence: the gate covers the whole
   engine, not one script. Alternative: reopen R5 for the scripting runtime
   only. Consequence: it misses physics, delta time, streaming and
   containers (§3.2), most of the risk.
9. **Q9 — Build compatibility.** Recommended: identical engine build on every
   peer in v1 (COOP-SES-005). Alternative: a versioned wire protocol that
   admits compatible builds. Consequence: protocol evolution rules and a
   compatibility test matrix from day one.
10. **Q10 — Who is "the player".** Recommended: `Game.GetPlayer()`, the
    `Player` reference and ObScript `player` mean the host's character;
    event arguments name the acting peer (COOP-SCR-002). Consequence: a
    guest can act in quests, but "player-only" conditions credit the host
    (§6 row 2). Alternative: substitute the acting peer as the player for
    the duration of its event's script run. Consequence: guests are
    credited, and two concurrent runs disagree about who the player is.

---

## 11. Document map

| Need | Go to |
|---|---|
| Entity ids, scheduler stages, parallel batches, declared access | [ecs.md](../engine/ecs.md), [§ Declared access](../engine/ecs.md#declared-access-r7--m27) |
| Delta time, pause and loading paths in the frame driver | [game-loop.md](../engine/game-loop.md) |
| Fixed physics tick and accumulator | [physics.md § Fixed tick](../engine/physics.md#fixed-tick-with-accumulator) |
| Script runtime, frame budget, real-time reads, encounter level | [scripting.md](../engine/scripting.md) |
| Snapshot contents, validation gates, live load-apply | [save-load-roundtrip.md](../engine/save-load-roundtrip.md) |
| FormID-keyed identity | [stream-boundary-state-continuity.md](../engine/stream-boundary-state-continuity.md) |
| Streaming around the player | [exterior-grid-streaming.md](../engine/exterior-grid-streaming.md) |
| Extension capability grants | [sandboxed-linked-mods.md](../engine/sandboxed-linked-mods.md) |
| Boot contract for host and join entry points | [launcher.md](../engine/launcher.md) |
| Per-game tiers | [game-compatibility.md](../engine/game-compatibility.md) |
| Plugin identity and Form IDs | [plugin-loading.md](../engine/plugin-loading.md) |
| VATS modes and world time scale | [VATS PRD](vats.md) |
| The milestone row, anti-scope, keep list, published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: §3.2 HEAD corrections (particle seed, `TotalTime`, Rapier SIMD, pause surface), interpreter-only frame budget, §3.4 connectivity routes with a server disposition each, community claims limited to own documents, VATS PRD link and FO4 slow motion, single-observable splits (DET-007, REP-009, SCR-007, TIME-005), runnable A1/A3, VRAM and netem caveats, ROADMAP co-op table as metric home | claude: revise |
