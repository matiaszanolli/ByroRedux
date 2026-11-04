# LLM Dialogue Plugin — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review round
1 (HEAD inventory, community-mod and licence citations, dialogue surface handed
to the Game UI Integration PRD, closed reason codes, ROADMAP wording
deviations, tightening) into the first draft of the M62 promise. Statements
about how an original engine, a model licence, an inference runtime or a
community mod behaves cite external sources (the CK / GECK wikis under
`/mnt/data/src/reference/`, licence texts, project READMEs); a claim that could
not be verified is marked **Phase 0: verify** instead of being stated as fact.

**Scope**: What it means for a ByroRedux mod to give NPCs locally generated
dialogue: who turns it on and for which speakers, where inference runs, how a
generated line reaches the player next to authored ones, and what it can never
do (advance a quest, run a fragment, reach the network). In scope: the
sandboxed extension that owns persona, prompt and memory; an engine-owned local
inference service; the dialogue, voice, save and diagnostics contracts that
carry its output. Out of scope: the dialogue menu surface (Game UI Integration
PRD), shipping any model with the engine, hosted inference, cloned real voices,
generated quests or records, and Fallout 76.

**Related**: this document states the *promise*; the designs below own the
*how*. The local inference service and the dialogue extension interface have no
design yet; Phase 0 writes one.

| Mechanism | Owning design |
|---|---|
| Activation conversations, INFO selection, line lifetime, voice | [Dialogue Trees](../engine/dialogue-trees.md) |
| Which surface draws the dialogue menu, and what it carries | [Game UI Integration PRD](game-ui-integration.md) (GUI-PAGE-004, its Q1) |
| Sandbox, capabilities, host interfaces, extension state | [Sandboxed Linked Mods](../engine/sandboxed-linked-mods.md) |
| SDK events, scheduling, semantic service catalog | [SDK v0.1 Development Plan](../engine/sdk-v0.1-development-plan.md) |
| Condition evaluator, quest stages, custom events, Papyrus VM | [Scripting](../engine/scripting.md) |
| Extension state in the engine save | [Save/Load Round-Trip](../engine/save-load-roundtrip.md#engine-native-extension-state) |
| VRAM targets and the live budget | [Memory Budget](../engine/memory-budget.md#vram-rough-budget-rtx-4070-ti-typical-fnv-interior) |
| Native-plugin dispositions and preflight | [Legacy Mod Compatibility PRD](legacy-mod-compatibility.md) |
| Local inference service, dialogue extension interface | Phase 0 writes `docs/engine/llm-dialogue.md` |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings,
as in the related designs. Requirement IDs use the `LLMD-` prefix.

---

## 1. Product thesis

ROADMAP files M62 under [Tier
9](../../ROADMAP.md#tier-9--better-than-bethesda-capability-stretch-post-tier-4-horizon),
the capability stretch "Bethesda demonstrably cannot ship on top of the Papyrus
stack-VM". The row's own words are the owner's framing: an "optional plugin
wiring DIAL / INFO + custom Papyrus events to a local LLM (Llama 3.x via candle
/ mistral.rs)", "off by default, opt-in per quest or per NPC", that "lives
entirely in plugin space", makes "no network calls", and whose "LLM weights
ship with the mod". Its dependencies, M47.0 (event hooks) and M43 (quests and
dialogue), are in ROADMAP's completed list and closed (2026-10-30)
respectively, and the tier's "no active work until M40 and M45" gate is met:
both are in the completed list.

It is for **mod authors** who want a companion or a town to answer the player
in its own words without a native DLL and a side server, and for **players**
who want such a mod to stay private, off the network, cheap on frame time and
harmless to the quest they are on.

The community already built this on the original engines. Mantella "is a Skyrim
and Fallout 4 mod which allows you to naturally speak to NPCs"; it "requires
Python 3.11", runs speech-to-text (Whisper, Moonshine) and "text-to-speech
(Piper / xVASynth / XTTS)", and lets the user "run everything locally on your
own hardware, or plug in OpenAI, OpenRouter, or any OpenAI-compatible API"; it
is AGPL-3.0 ([Mantella repo](https://github.com/art-from-the-machine/Mantella),
[licence](https://github.com/art-from-the-machine/Mantella/blob/main/LICENSE),
[Mantella site](https://art-from-the-machine.github.io/Mantella/)). CHIM's
HerikaServer "serves as a bridge between the SKSE plugin and various AI
providers", naming "ChatGPT, MeloTTS, koboldcpp, Openrouter, XTTS"
([HerikaServer](https://github.com/language-coder/HerikaServer)). Both run an
external process beside the game and can route to hosted APIs; CHIM also needs
a native extender plugin. ByroRedux can offer the experience as a sandboxed mod
with no extender, no side process and no socket.

The corollary that shapes every requirement below: **a generated line is
presentation, never authority.** It may be shown, voiced and remembered by the
mod that made it; it never selects an authored INFO, runs a fragment, changes a
quest stage, writes engine state, or reaches anything outside the user's
machine. Everything a player can do to the world in a conversation still goes
through an authored topic the player chose, and the generated lane draws no UI
of its own: it rides whatever surface the Game UI Integration PRD chooses.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Dialogue extension** | A sandboxed executable mod ([extension component](../engine/sandboxed-linked-mods.md#3-terminology)) that requests the dialogue capabilities of §5.2. It owns persona text, prompt assembly, conversation memory and eligibility. |
| **Inference service** | The engine-owned host service that loads a model the extension's package ships and runs generation for it off the scheduler thread (§5.3). It knows nothing about dialogue. |
| **Model artifact** | A weight file (with its tokenizer and licence file) a package declares as content. Never shipped with the engine. |
| **Eligible speaker** | An NPC for which a granted dialogue extension may produce lines: named in its manifest directly, or bound by a manifest-named quest that is running (§5.1). |
| **Authored line** | A line from a `DIAL` / `INFO` record, selected by the dialogue selection code exactly as at HEAD. |
| **Generated line** | Text produced by the inference service for a dialogue extension and presented on the dialogue surface. |
| **Dialogue surface** | Whatever draws the dialogue menu: the native page today, or the surface the Game UI Integration PRD's Q1 picks (GUI-PAGE-004). |
| **Provenance** | Whether a presented line is authored or generated; reported on the surface and by `dialogue.status`. |
| **Generation request** | One ticket from the extension to the inference service: prompt, sampling settings, token ceiling, deadline. |
| **Fallback** | What the surface shows when a request fails, times out or is refused: the authored conversation as it would be without the extension. |
| **Headroom** | Device-local memory the engine is not using, read from the live budget (`VK_EXT_memory_budget`) minus a margin (§5.7). |

---

## 3. Core model

A conversation with an eligible speaker has two lanes. The **authored lane** is
today's conversation, unchanged: topics, INFO selection, fragments, voice and
Say-Once bookkeeping. The **generated lane** adds lines beside it. The player
still acts only through the authored lane; the generated lane only talks.

Each generation request is in one of these states, and the state is what the
surface and `dialogue.status` report:

| State | Meaning | Reported as |
|---|---|---|
| **Pending** | Ticket accepted, no text yet | surface cue "(thinking)"; `generation=pending` |
| **Streaming** | Text arriving; subtitle grows | "(speaking, generated)"; `generation=streaming` |
| **Presented** | Complete line shown | provenance tag "generated"; `generation=presented` |
| **Fallen back** | The request ended without a line; the authored lane continues | `generation=fallback reason=<code>` |
| **Refused** | The request never reached generation; nothing shown | `generation=refused reason=<code>` |

The reason codes are closed; a requirement that needs a new one revises this
table.

| Code | State | Raised by |
|---|---|---|
| `timeout` | Fallen back | deadline exceeded (LLMD-INF-005) |
| `fault` | Fallen back | backend error (LLMD-INF-007) |
| `empty` | Fallen back | zero tokens produced |
| `cancelled` | Fallen back | topic chosen or surface closed (LLMD-DLG-005, -008) |
| `not-granted` | Refused | LLMD-OPT-001 |
| `not-eligible` | Refused | LLMD-OPT-004 |
| `lane-off` | Refused | LLMD-OPT-005 |
| `quota` | Refused | LLMD-INF-006 |
| `hash-mismatch` | Refused | LLMD-INF-003 |
| `no-headroom` | Refused | LLMD-RES-001 |
| `surface-unsupported` | Refused | LLMD-DLG-002 |

Provenance is binary and closed: every line on the surface is **authored** or
**generated**. There is no third kind and no way for an extension to present a
generated line as authored.

### 3.1 What exists at HEAD

State at HEAD on 2026-11-01, read from the code cited in each row. The
hand-written "Today" column is replaced in Phase 0 by the design's links to the
tests that pin each row.

| # | Piece | Needed for | Today |
|---|---|---|---|
| 1 | Activation conversation | the moment a line can be generated | `open_conversation` and `apply_selection` in `byroredux/src/systems/npc_dialogue.rs`: blocking branch, top-level menu, generic-greeting fallback (`generic_greeting_record`) |
| 2 | Condition-gated INFO choice | the authored lane | `select_first_info` (public) over the private `select_info` in `crates/scripting/src/dialogue.rs`, on the M47.1 `evaluate` (`crates/scripting/src/condition/eval.rs`) |
| 3 | INFO fragments | the authority the generated lane must not touch | `apply_selection` runs the outgoing line's OnEnd and the new line's OnBegin (`speak_info_begin_fragment`) |
| 4 | Native dialogue page | today's dialogue surface | `draw_dialogue_page` in `crates/debug-ui/src/panels/game_menu.rs`; player input is `DialogueUiAction::SelectTopic` (its only variant) and the "Close (Esc)" button; no text input |
| 5 | Voice and speaking cue | voicing or not voicing generated text | `play_line_voice` in `byroredux/src/systems/dialogue_voice.rs`; `DialogueSurfaceState::line_voiced` (`crates/scripting/src/dialogue.rs`) drives "(speaking, no voice)" |
| 6 | Saved dialogue state | what generated text must stay out of | `DialogueSpokenInfoForms` (`crates/scripting/src/dialogue.rs`), registered in `byroredux/src/save_io/registry.rs` |
| 7 | Sandbox with no OS access | "no network calls" | No WASI linked (`crates/mod-runtime/src/lib.rs` module doc); the `extension` world in `crates/mod-runtime/wit/host.wit` has no network, filesystem, dialogue, quest or task interface |
| 8 | Read interfaces the dialogue context can reuse | LLMD-HOST-002 | `world-state.get-entity` returns the entity's `name`; `factions.get` returns memberships and ranks (`host.wit`); nothing reads race, location name or quest objectives |
| 9 | Guest ceilings | why inference cannot run in the guest | `SandboxConfig::default` in `crates/mod-runtime/src/limits.rs` (component bytes, linear memory, fuel per entry) |
| 10 | Extension loading and grants | opt-in | `--extension` and `--extension-grant <id>=<capability>` in `byroredux/src/extensions/install.rs` |
| 11 | Capability descriptions | what the user is told on a grant | `CAPABILITY_DESCRIPTORS` in `crates/mod-runtime/src/runtime/sandbox.rs` ("the description a mod manager shows the player") |
| 12 | Principal storage in the save | conversation memory | `PrincipalStorageLimits::default` (`crates/sdk/src/storage.rs`, `max_total_bytes_per_principal`), carried in the extension payload ([save design](../engine/save-load-roundtrip.md#engine-native-extension-state)) |
| 13 | Package manifest | declaring a model artifact | `ExtensionManifest` (`crates/sdk/src/manifest.rs`, `deny_unknown_fields`): components, capabilities, subscriptions, schemas, commands, script functions, settings; no content-artifact list ([ARC-002](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment) unbuilt) |
| 14 | Script-side triggers | "custom Papyrus events" | SKSE `ModEvent` both ways (`queue_papyrus_mod_event`, `crates/scripting/src/papyrus_provider/ir.rs`; publisher wired in `byroredux/src/extensions/systems.rs`); extensions publish Papyrus-callable functions (`script-functions`, `on-script-function`); FO4 `SendCustomEvent` stays inside the VM (`crates/scripting/src/fragment/events.rs`) |
| 15 | VRAM telemetry | contention with the renderer | `query_live_memory_budget` (`crates/renderer/src/vulkan/allocator.rs`); `MetricsPayload::vulkan_heap_used_mb` and `driver_vram_used_mb` in `crates/debug-protocol/src/lib.rs` |
| 16 | Inference runtime | generation | None: no inference crate in the workspace |
| 17 | C++ interop | a llama.cpp backend (Q2) | `crates/cxx-bridge` exists but holds one placeholder function (`native_hello`, `src/lib.rs`) |

---

## 4. Goals and non-goals

### 4.1 Goals

- **LLMD-G-001 — Opt-in, nothing by default.** With no dialogue extension
  granted, every conversation is today's: the existing dialogue unit tests and
  `dt1-dialogue-layers.sh` pass unchanged.
- **LLMD-G-002 — Authority stays authored.** Over the acceptance scenarios,
  zero quest-stage, objective, fragment or saved-dialogue-state changes are
  caused by a generated line.
- **LLMD-G-003 — Local only.** A dialogue extension's whole lifetime opens zero
  network sockets and reads no file outside its package.
- **LLMD-G-004 — The renderer keeps its frame.** Generation beside a benched
  scene moves p95 frame time by no more than the Q6 threshold.
- **LLMD-G-005 — Answers arrive in conversation time.** Time to first visible
  text and to a complete line are published per model class and hardware
  against the Q6 targets.
- **LLMD-G-006 — Honest provenance.** Every generated line is labelled as
  generated on the surface and in diagnostics.
- **LLMD-G-007 — One reference mod.** A sample dialogue extension, built on the
  SDK with a model chosen per Q7, passes the §7.2 scenarios on the wave-1
  games.

### 4.2 Non-goals

- **LLMD-NG-001.** Shipping, downloading or updating any model with the engine
  (licence terms vary, §5.10). Where the weights come from for a mod is Q7.
- **LLMD-NG-002.** Cloud, hosted or networked inference, and any local server
  reached over a socket, even with a network grant: the ROADMAP row says "no
  network calls" and anti-scope bars hosted services ([What we are NOT
  doing](../../ROADMAP.md#what-we-are-not-doing-anti-scope)), which is stricter
  than the sandbox's grant-gated network
  ([SEC-004](../engine/sandboxed-linked-mods.md#102-requirements)).
- **LLMD-NG-003.** Generated quests, stages, records, items or packages (§1
  corollary).
- **LLMD-NG-004.** Synthesising the voice of a real person, including the
  games' voice actors, from their recordings.
- **LLMD-NG-005.** Loading a legacy AI-dialogue mod's native plugin or its side
  server ([LMC-NG-001](legacy-mod-compatibility.md#42-non-goals),
  [LMC-EXT-001](legacy-mod-compatibility.md#55-native-extensions-lmc-ext));
  such a mod is ported to a dialogue extension.
- **LLMD-NG-006.** Engine-side persona or prompt logic: personas, prompt
  templates and memory belong to the extension ("lives entirely in plugin
  space"); the engine owns only generic services.
- **LLMD-NG-007.** Fine-tuning or training inside the engine.
- **LLMD-NG-008.** Any dialogue UI work: which surface draws the menu, its
  layout and its input widgets are the Game UI Integration PRD's (GUI-PAGE-004
  and its Q1). This PRD states what the generated lane needs a surface to carry
  (§5.4) and refuses where a surface cannot.
- **LLMD-NG-009.** Fallout 76, out of scope as in the Legacy Mod Compatibility
  PRD's Scope and [LMC-NG-008](legacy-mod-compatibility.md#42-non-goals).

---

## 5. Requirements by area

### 5.1 Opt-in and eligibility (`LLMD-OPT`)

- **LLMD-OPT-001.** A dialogue extension MUST produce no line unless the user
  granted it the dialogue capabilities of §5.2 through the existing grant path
  (`--extension-grant` at HEAD, the launcher's profile once it carries grants);
  an ungranted request MUST return `refused reason=not-granted`.
- **LLMD-OPT-002.** The manifest MUST declare eligible speakers as a list of
  portable form references (NPC base or placed reference) and a list of quests;
  no other route makes a speaker eligible.
- **LLMD-OPT-003.** A manifest-named quest MUST make a speaker eligible only
  while `running_quests_binding_entity`
  (`crates/scripting/src/scene/quest_alias.rs`, the edge the authored lane uses
  in `npc_dialogue.rs` at HEAD on 2026-11-01) lists that quest for the speaker.
- **LLMD-OPT-004.** A request for an ineligible speaker MUST return `refused
  reason=not-eligible` and MUST NOT reach the inference service.
- **LLMD-OPT-005.** A native setting MUST switch the generated lane off per
  extension without uninstalling it; with it off, requests return `refused
  reason=lane-off` from the next conversation open.
- **LLMD-OPT-006.** An eligible speaker with the lane on MUST still open its
  authored conversation first (LLMD-DLG-001); the extension adds lines and
  never replaces the opening.

### 5.2 Plugin boundary and host interfaces (`LLMD-HOST`)

Each addition is a new versioned interface, separately capability-gated
([API-005](../engine/sandboxed-linked-mods.md#15-api-evolution-and-compatibility),
[API-008](../engine/sandboxed-linked-mods.md#15-api-evolution-and-compatibility)),
and reuses the read interfaces of §3.1 row 8 instead of duplicating them.

- **LLMD-HOST-001.** A dialogue event MUST be delivered to the subscribed,
  granted dialogue extensions when a conversation opens with an eligible
  speaker and when the authored line changes, carrying the speaker's entity
  reference, the selected topic and INFO form references, the authored response
  text and the offered topic list.
- **LLMD-HOST-002.** For a speaker disclosed in the current callback, a
  dialogue extension MUST be able to read race, location name and the displayed
  objectives of running quests that bind it through new read-only snapshot
  calls, and name and factions through the existing `world-state` and
  `factions` interfaces; it MUST NOT be able to read an unbound quest's text.
- **LLMD-HOST-003.** Player-typed text (only if Q3 admits it) MUST reach only
  the dialogue extension that owns the current generated lane, bounded to a
  byte ceiling the interface declares.
- **LLMD-HOST-004.** The extension MUST submit a generated line as a deferred
  command applied at a scheduler barrier
  ([ECS-002](../engine/sandboxed-linked-mods.md#111-host-service-families)); a
  command naming a speaker no longer in conversation MUST be rejected and
  logged.
- **LLMD-HOST-005.** Generation MUST be a ticketed task: the request call
  returns a ticket, and completion or each streamed chunk arrives as a later
  callback
  ([ECS-014](../engine/sandboxed-linked-mods.md#111-host-service-families)); no
  guest call MAY block waiting for inference.
- **LLMD-HOST-006.** The interfaces this PRD adds MUST NOT expose a quest,
  stage, objective, INFO selection, fragment, package, inventory, actor value
  or record mutation; a test over the WIT world's imports checks it (§7.2
  scenario 4).
- **LLMD-HOST-007.** Each dialogue capability MUST have a user-facing entry in
  `CAPABILITY_DESCRIPTORS` (§3.1 row 11).

### 5.3 Inference service (`LLMD-INF`)

Inference cannot run in the guest at today's ceilings (§3.1 row 9: MiB of
linear memory against GB of weights, and no GPU access). Wasmtime's `wasi-nn`
crate "is experimental and its API, functionality, and location could quickly
change" and "currently … contains an implementation of wasi-nn using OpenVINO"
([wasmtime-wasi-nn
README](https://github.com/bytecodealliance/wasmtime/tree/main/crates/wasi-nn)).
Where inference runs is Q1; the requirements below hold for the recommended
answer.

- **LLMD-INF-001.** The service MUST run generation on engine-owned worker
  threads, never on the scheduler or render thread (§7.2 scenario 5).
- **LLMD-INF-002.** The service MUST load only a model artifact declared in the
  requesting package's manifest, resolved package-relative under the
  content-read confinement of
  [SEC-018](../engine/sandboxed-linked-mods.md#102-requirements); manifest
  validation MUST refuse a path outside the package.
- **LLMD-INF-003.** The service MUST verify the artifact against the manifest's
  content hash before loading
  ([ARC-006](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment)),
  refusing a mismatch with `refused reason=hash-mismatch`.
- **LLMD-INF-004.** The service MUST NOT open a socket, spawn a process or read
  a path the manifest did not declare.
- **LLMD-INF-005.** Each request MUST carry a token ceiling and a deadline,
  both clamped to service maxima; exceeding the deadline MUST end it in
  `fallback reason=timeout`.
- **LLMD-INF-006.** Each principal MUST have at most one request in flight per
  speaker and a bounded queue; an over-quota request MUST return `refused
  reason=quota`
  ([SCALE-013](../engine/sandboxed-linked-mods.md#14-performance-and-scale)).
- **LLMD-INF-007.** A backend fault MUST end the request in `fallback
  reason=fault` without terminating the engine
  ([SEC-011](../engine/sandboxed-linked-mods.md#102-requirements)).
- **LLMD-INF-008.** The backend MUST sit behind an engine-owned trait so the
  first backend (Q2) is replaceable without changing the host interface, and
  the test suite MUST include a deterministic stub backend that needs no model
  file.
- **LLMD-INF-009.** The feature MUST NOT require a proprietary GPU toolkit at
  build or run time; a vendor-specific backend MAY be an optional build feature
  (Q2).
- **LLMD-INF-010.** A request with a fixed seed and greedy sampling on the same
  backend, model and hardware SHOULD reproduce its tokens. **Phase 0: verify**
  per Q2 candidate by running one prompt twice and diffing the tokens.

### 5.4 Dialogue integration and presentation (`LLMD-DLG`)

What the generated lane needs from the dialogue surface; how a surface draws it
is the Game UI Integration PRD's (GUI-PAGE-004 carries the native page's cue,
subtitle, replies and said-markers to whichever surface its Q1 picks).

- **LLMD-DLG-001.** On an eligible speaker the authored lane MUST open and
  select exactly as at HEAD (`open_conversation`), and its line MUST be
  presented before any generated line.
- **LLMD-DLG-002.** A generated line MUST be shown with a visible "generated"
  provenance tag; on a surface that cannot carry the tag, the request MUST
  return `refused reason=surface-unsupported`.
- **LLMD-DLG-003.** While a request is pending the surface MUST show the
  "(thinking)" cue, and while text streams the subtitle MUST grow in place.
- **LLMD-DLG-004.** A generated line MUST NOT add, remove, reorder or relabel
  an offered topic: the topic list equals the authored list.
- **LLMD-DLG-005.** Selecting an authored topic while a request is pending or
  streaming MUST end it in `fallback reason=cancelled` and proceed on the
  authored lane.
- **LLMD-DLG-006.** `dialogue.status` (`byroredux/src/commands/quest.rs` at
  HEAD on 2026-11-01) MUST report each presented line's provenance and, for a
  generated line, its state and owning principal.
- **LLMD-DLG-007.** A fallen-back or refused request MUST leave the surface
  identical to the authored lane alone.
- **LLMD-DLG-008.** Closing the surface MUST end the speaker's in-flight
  request in `fallback reason=cancelled`.
- **LLMD-DLG-009.** The generated lane MUST read only the canonical dialogue
  data the authored lane already resolved (`DialRecord`, `InfoRecord`, the
  selection) and MUST NOT re-derive per-game differences (greeting topic, voice
  path, branch flags), which stay in the existing selection and voice code.

### 5.5 Quest and world safety (`LLMD-SAFE`)

In the original games dialogue advances quests through the INFO's scripts:
"Topic Info fragments are run when the topic begins, and when the topic ends",
and the CK tutorial's End fragment is `GetOwningQuest().SetStage(10)` ([CK:
Topic Info Fragments](https://ck.uesp.net/wiki/Topic_Info_Fragments), [CK:
Bethesda Tutorial
Dialogue](https://ck.uesp.net/wiki/Bethesda_Tutorial_Dialogue)); on FO3/FNV
"Result Script (Begin): This script runs when the Info is played" ([GECK:
Category:Dialogue](https://geck.uesp.net/wiki/Category:Dialogue)). The
generated lane therefore never touches INFO selection or fragments.

- **LLMD-SAFE-001.** Presenting a generated line MUST NOT dispatch any INFO
  fragment or result script (§7.2 scenario 3).
- **LLMD-SAFE-002.** A generated exchange MUST leave `QuestStageState`, quest
  objectives and quest running state equal before and after.
- **LLMD-SAFE-003.** A generated line MUST NOT enter `DialogueSpokenInfoForms`
  or change Say-Once, Random or Goodbye selection on the authored lane.
- **LLMD-SAFE-004.** Model output MUST NOT select an authored INFO; only a
  player action on an authored topic MAY (Q5 revises this ID if it admits a
  router mode).
- **LLMD-SAFE-005.** No condition function MAY read generated text or
  generation state.
- **LLMD-SAFE-006.** A fixture conversation run with and without the generated
  lane MUST produce equal save snapshots once the extension's principal storage
  is excluded, so a replay that ignores the lane (the M60 co-op model) diverges
  in presentation only.

### 5.6 Content safety and disclosure (`LLMD-CONT`)

Model licences can bind the mod's use. The Llama 3 and 3.2 Acceptable Use
Policies prohibit "representing that the use of Meta Llama 3 or outputs are
human-generated" (3.2: "of Llama 3.2"), content "intended to incite or promote
violence", and failing "to appropriately disclose to end users any known
dangers of your AI system" ([Llama 3
AUP](https://dev.meta.ai/llama/llama3/use-policy/), [Llama 3.2
AUP](https://dev.meta.ai/llama/llama3_2/use-policy/)). The provenance tag
answers the first; whether in-fiction game violence falls under the second is a
reading for Q7.

- **LLMD-CONT-001.** Every generated line MUST carry the provenance tag
  (LLMD-DLG-002); no interface MAY let an extension remove it.
- **LLMD-CONT-002.** Generated text MUST be rendered as tagged plain mod text
  with control sequences stripped
  ([SEC-015](../engine/sandboxed-linked-mods.md#102-requirements)).
- **LLMD-CONT-003.** The manifest MUST declare a content rating from a closed
  list fixed in Phase 0, shown with the grant.
- **LLMD-CONT-004.** The player MUST be able to hide a presented generated line
  (the control is the surface's, GUI-PAGE-004); hiding removes it and passes a
  "hidden" signal to the owning extension.
- **LLMD-CONT-005.** The engine MUST NOT log generated or player-typed text
  above debug level, nor write either to any file outside the save's extension
  payload.

### 5.7 Resource budgets (`LLMD-RES`)

VRAM targets, including the RT-minimum and dev cards, are owned by [Memory
Budget](../engine/memory-budget.md#vram-rough-budget-rtx-4070-ti-typical-fnv-interior);
a GPU-resident model competes for what the renderer leaves.

- **LLMD-RES-001.** Before placing a model on the GPU the service MUST read the
  live device-local budget and usage and, when the model's declared footprint
  exceeds headroom minus the Q6 margin, MUST place it on the CPU if Q2's
  backend can, else return `refused reason=no-headroom`.
- **LLMD-RES-002.** GPU memory the backend allocates MUST be visible in the
  `byro-dbg` metrics. **Phase 0: verify** whether allocations made through a
  second API or a second Vulkan device appear in `vulkan_heap_used_mb` or only
  in `driver_vram_used_mb`, by allocating a known size in a standalone backend
  run beside the engine and reading both.
- **LLMD-RES-003.** A renderer allocation failure while a model is resident
  MUST unload the model and log it before the renderer degrades.
- **LLMD-RES-004.** CPU inference MUST run on a worker pool whose size is a
  setting.
- **LLMD-RES-005.** With a request streaming, frame-time p95 on a pinned bench
  scene MUST stay within the Q6 threshold of a same-session control (§7.2
  scenario 9).
- **LLMD-RES-006.** First-text and complete-line latency MUST be published as
  p50 and p95 per backend, model class and hardware (§8).
- **LLMD-RES-007.** A model MUST load no earlier than its extension's first
  eligible conversation in the session (or at boot when the "preload" setting
  is on) and MUST unload when the lane is switched off.

### 5.8 Voice (`LLMD-VOX`)

At HEAD a line with no resolvable voice keeps the subtitle estimate and shows
"(speaking, no voice)" (§3.1 row 5). Synthetic voice is Q4.

- **LLMD-VOX-001.** A generated line without synthesis MUST present with
  `line_voiced = false` and the estimate-based window the authored lane uses.
- **LLMD-VOX-002.** A generated line MUST NOT play any authored voice file.
- **LLMD-VOX-003.** If Q4 admits synthesis, the voice MUST come from the
  extension's own package under the confinement of LLMD-INF-002 to -004 and
  play through the authored voice's spatial path at the speaker.
- **LLMD-VOX-004.** If Q4 admits synthesis, the manifest MUST declare each
  voice's licence, shown with the grant.

### 5.9 Persistence (`LLMD-SAV`)

- **LLMD-SAV-001.** Generated text MUST NOT be written to any engine-owned save
  column or resource; the only persisted trace is the extension's principal
  storage
  ([STATE-001](../engine/sandboxed-linked-mods.md#13-persistent-state-and-saves),
  quota in §3.1 row 12).
- **LLMD-SAV-002.** This PRD MUST NOT require a `FORMAT_MAJOR` bump; an
  extension-state format change goes through the extension payload's own
  version.
- **LLMD-SAV-003.** Loading a save made with a dialogue extension, with that
  extension absent, MUST succeed with its state retained opaque (the design's
  SHOULD,
  [STATE-008](../engine/sandboxed-linked-mods.md#13-persistent-state-and-saves),
  made a MUST here) and every conversation authored-only.
- **LLMD-SAV-004.** A native setting MUST purge one extension's stored
  conversation memory without editing the save (STATE-008's "purge orphan
  state").
- **LLMD-SAV-005.** An in-flight request MUST NOT be saved; a save during
  generation records the conversation as authored-only.

### 5.10 Licensing and distribution (`LLMD-LIC`)

Every row below was read from the linked licence text on 2026-11-01.

| Artifact | Licence | Terms that bind a mod | Source |
|---|---|---|---|
| Llama 3 | Meta Llama 3 Community License (release date April 18, 2024) | redistributors "provide a copy of this Agreement", "prominently display 'Built with Meta Llama 3'", and "include 'Llama 3' at the beginning of any such AI model name"; a licensee whose products exceeded "700 million monthly active users" on the release date "must request a license from Meta" | [licence](https://dev.meta.ai/llama/llama3/license/) |
| Llama 3.1 | Llama 3.1 Community License (July 23, 2024) | as Llama 3, with "Built with Llama" and "Llama" as the name prefix | [licence](https://dev.meta.ai/llama/llama3_1/license/) |
| Llama 3.2 | Llama 3.2 Community License (September 25, 2024) | as 3.1; its AUP withholds the Section 1(a) rights for "any multimodal models included in Llama 3.2" from EU-domiciled individuals and companies, which "does not apply to end users" | [licence](https://dev.meta.ai/llama/llama3_2/license/), [AUP](https://dev.meta.ai/llama/llama3_2/use-policy/) |
| Mistral 7B | Apache-2.0 ("We're releasing Mistral 7B under the Apache 2.0 license, it can be used without restrictions", 2023-09-27) | Apache-2.0 notice and licence copy | [announcement](https://mistral.ai/news/announcing-mistral-7b), [model card](https://huggingface.co/mistralai/Mistral-7B-v0.1), [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) |
| Qwen2.5 except 3B / 72B | Apache-2.0 ("All our open-source models, except for the 3B and 72B variants, are licensed under Apache 2.0", 2024-09-19) | Apache-2.0 notice and licence copy | [announcement](https://qwenlm.github.io/blog/qwen2.5/), [7B LICENSE](https://huggingface.co/Qwen/Qwen2.5-7B-Instruct/blob/main/LICENSE) |
| Qwen2.5 3B | Qwen Research License Agreement | modification and distribution "FOR NON-COMMERCIAL PURPOSES ONLY" ("research or evaluation purposes only") | [LICENSE](https://huggingface.co/Qwen/Qwen2.5-3B-Instruct/blob/main/LICENSE) |
| Qwen2.5 72B | Qwen License Agreement | commercial use with "more than 100 million monthly active users" needs a licence from Alibaba | [LICENSE](https://huggingface.co/Qwen/Qwen2.5-72B-Instruct/blob/main/LICENSE) |
| candle | Apache-2.0 or MIT | — | [LICENSE-MIT](https://github.com/huggingface/candle/blob/main/LICENSE-MIT), [LICENSE-APACHE](https://github.com/huggingface/candle/blob/main/LICENSE-APACHE) |
| mistral.rs | MIT | — | [LICENSE](https://github.com/EricLBuehler/mistral.rs/blob/master/LICENSE) |
| llama.cpp | MIT | — | [LICENSE](https://github.com/ggml-org/llama.cpp/blob/master/LICENSE) |
| Piper (TTS, Q4) | the original repository is MIT and "was archived by the owner on Oct 6, 2025"; development moved to `piper1-gpl`, which is GPL-3.0; each voice's `MODEL_CARD` carries its own licence ("Some voices may have restrictive licenses") | GPL-3.0 code cannot be linked into the MIT engine (LLMD-LIC-005); voices are checked one by one | [old LICENSE](https://github.com/rhasspy/piper/blob/master/LICENSE.md), [COPYING](https://github.com/OHF-Voice/piper1-gpl/blob/main/COPYING), [VOICES.md](https://github.com/OHF-Voice/piper1-gpl/blob/main/docs/VOICES.md) |
| ByroRedux | MIT | — | `Cargo.toml` `[workspace.package] license` |

- **LLMD-LIC-001.** The manifest MUST declare, per model artifact, its licence
  identifier, licence file path and any attribution string the licence
  requires.
- **LLMD-LIC-002.** The host MUST display the declared attribution (for example
  "Built with Llama") wherever the extension is listed and when the grant is
  requested.
- **LLMD-LIC-003.** Manifest validation MUST refuse a model artifact whose
  declared licence file is missing from the package.
- **LLMD-LIC-004.** The engine repository MUST NOT contain model weights,
  tokenizers from a model distribution, or synthetic voices; tests use the stub
  backend (LLMD-INF-008) and real-model runs read a user-supplied path.
- **LLMD-LIC-005.** No AGPL- or GPL-licensed code (Mantella, `piper1-gpl`) MAY
  be copied into or linked with the engine or the reference mod; precedent is
  cited, not ported.

### 5.11 Script triggers (`LLMD-SCR`)

- **LLMD-SCR-001.** An authored Skyrim script MUST be able to request a line
  through an SKSE `ModEvent` the extension subscribes to, over the bridge at
  HEAD (§3.1 row 14); the line arrives on the dialogue surface or, outside a
  conversation, as a subtitle at the speaker.
- **LLMD-SCR-002.** A Papyrus-callable script function a dialogue extension
  publishes (`script-functions`) MUST return a ticket or status synchronously,
  never a generated string (LLMD-HOST-005).
- **LLMD-SCR-003.** An FO4 `SendCustomEvent`, which "returns immediately, the
  receivers receiving the event as if it was a game event" ([FO4 CK:
  SendCustomEvent](https://falloutck.uesp.net/wiki/SendCustomEvent_-_ScriptObject)),
  SHOULD reach a subscribed dialogue extension through the `ModEvent`
  custom-event channel; at HEAD it stays inside the VM (§3.1 row 14), so this
  is new bridge work.
- **LLMD-SCR-004.** ObScript titles (Oblivion, FO3, FNV) MAY request a line
  through the compiled-SCDA SDK-call route ([SDK plan
  §6.6](../engine/sdk-v0.1-development-plan.md#66-semantic-service-catalog)).
- **LLMD-SCR-005.** A script-triggered line MUST satisfy every `LLMD-SAFE`
  requirement: the calling script keeps its own authored effects and the
  generated text adds none.

### 5.12 Diagnostics (`LLMD-DX`)

- **LLMD-DX-001.** A new `inference.status` console command MUST list each
  loaded model with owning principal, artifact hash, placement (CPU / GPU),
  resident bytes, requests served, timeouts and faults.
- **LLMD-DX-002.** Each refusal and fallback MUST be logged once per
  conversation with its §3 reason code and principal.
- **LLMD-DX-003.** The preflight report
  ([LMC-DX-001](legacy-mod-compatibility.md#511-tooling-and-explainability-lmc-dx))
  MUST list a dialogue extension's requested capabilities, model artifacts with
  licence and footprint, and content rating, without loading the model.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | AI-dialogue mods run an external process beside the game (Mantella's Python app; CHIM's server "between the SKSE plugin and various AI providers", §1) | A sandboxed extension calls an engine inference service (LLMD-INF-001) | The side process, CHIM's native plugin, their install steps | Ported mods lose features their server had (STT, many backends) → Q3, Q4; preflight states what a port lacks (LLMD-DX-003) |
| 2 | Those mods can route to hosted APIs (OpenAI, OpenRouter, §1) | Local only, no socket even with a grant (LLMD-INF-004, LLMD-NG-002) | API keys, per-token billing | Weak hardware gets slower or smaller models → latency published per hardware (LLMD-RES-006) |
| 3 | All dialogue is authored INFO text | Generated lines beside authored ones (LLMD-DLG-001) | — | A player mistakes generated text for quest guidance → provenance tag (LLMD-DLG-002, LLMD-CONT-001) |
| 4 | Dialogue advances quests through INFO fragments (§5.5) | Generated lines run no fragment (LLMD-SAFE-001) | — | A mod author expects "the NPC agreed, so the quest advances" → Q5; the answer is always an authored topic |
| 5 | Authored lines are voiced by actors | Generated lines are subtitle-only unless Q4 admits synthesis (LLMD-VOX-001) | — | Silent lines feel flat beside voiced ones; cloning excluded (LLMD-NG-004) |
| 6 | A side process uses its own GPU memory, invisible to the game | The engine budgets the model against live headroom (LLMD-RES-001) | Hand-tuned layer offload | Off-allocator memory may be invisible → Phase 0 verify (LLMD-RES-002) |
| 7 | Mantella / CHIM memory lives in the side process's files | Memory lives in the extension's principal storage (LLMD-SAV-001) | Per-playthrough memory files | The principal quota bounds memory length → the extension summarises; the quota is the sandbox's setting |
| 8 | ROADMAP M62: inference "in plugin space", "weights ship with the mod", "candle / mistral.rs" | An engine service runs inference (Q1); the reference mod may read user-supplied weights (Q7); the first backend may be llama.cpp (Q2) | — | The ROADMAP row's wording changes with Q1, Q2 and Q7; §10 states each consequence |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

The generated lane sits above the canonical dialogue data, so it carries no
per-game code (LLMD-DLG-009). A game is in a wave when its authored activation
conversation is live-gated on a dialogue surface ([smoke-test
index](../smoke-tests/README.md)).

| Game | In scope | Why / wave |
|---|---|---|
| Fallout: New Vegas | yes, wave 1 | Engine reference title; greeting and force-greet live-gated by `dt1-dialogue-layers.sh`; ObScript trigger route (LLMD-SCR-004) |
| Skyrim (LE / SE / AE) | yes, wave 1 (SE gated) | Mantella / CHIM precedent; `dt2-skyrim-forcegreet.sh` gates force-greet and generic greeting. **Phase 0: verify** that every `dt2` leg passes at HEAD (its generic-hello leg has failed as a baseline before), by running it on SE; `ModEvent` route exists (LLMD-SCR-001) |
| Fallout 3 | yes, wave 2 | Same dialogue machinery as FNV, but `dt1` declares FO3 a SKIP (no measured live route, #5429); joins when that route exists |
| Fallout 4 | yes, wave 2 | Mantella names it; needs the `CustomEvent` bridge (LLMD-SCR-003). **Phase 0: verify** that an FO4 activation conversation reaches the dialogue surface, by a live activation on a named FO4 NPC |
| Oblivion | yes, wave 3 | No quest aliases, so no NPC owns a topic yet (`npc_dialogue.rs` module doc). **Phase 0: verify** that the generic-greeting route opens an Oblivion conversation |
| Starfield | yes, wave 3 | **Phase 0: verify** that a Starfield activation conversation reaches the dialogue surface |
| Fallout 76 | no | LLMD-NG-009 |

### 7.2 Acceptance

Test names are new unless marked existing. A scenario that needs a model reads
it from a user-supplied path (`BYROREDUX_LLM_MODEL`, a file the user obtained
under its licence); nothing in the repository contains a model. Plumbing
scenarios use the stub backend.

| # | Scenario | Driver | Data | Pass rule |
|---|---|---|---|---|
| 1 | No extension, nothing changes | existing `dialogue_page_marks_speaking_replies_and_said_topics` (`crates/debug-ui/src/panels/game_menu.rs`), existing `docs/smoke-tests/dt1-dialogue-layers.sh` | FNV install for the smoke | Both pass unchanged (LLMD-G-001) |
| 2 | Ungranted and ineligible are refused | `cargo test -p byroredux --bin byroredux extensions::tests::dialogue_generation_refuses_ungranted_and_ineligible` | stub backend | `not-granted` and `not-eligible`; zero backend calls (LLMD-OPT-001, -004) |
| 3 | Authority stays authored | `extensions::tests::generated_exchange_runs_no_fragment_and_keeps_quest_state` | stub backend, fixture DIAL/INFO with OnBegin/OnEnd fragments | No fragment dispatched; `QuestStageState` and `DialogueSpokenInfoForms` equal before and after (LLMD-SAFE-001 to -003) |
| 4 | The WIT world exposes no mutation | `byroredux-mod-runtime` test `dialogue_interfaces_expose_no_world_mutation` | none | The new interfaces' functions match a checked-in allow-list (LLMD-HOST-006) |
| 5 | Generation never blocks a frame | `extensions::tests::pending_generation_does_not_block_the_scheduler_pass` | stub backend with a 2 s sleep | The scheduler pass returns before the stub completes (LLMD-INF-001) |
| 6 | Provenance on the surface | `debug-ui` test `dialogue_page_tags_generated_lines`, plus its twin on the surface GUI Q1 picks | snapshot fixture | "generated" tag and "(thinking)" cue drawn; topic list equals the authored one (LLMD-DLG-002 to -004) |
| 7 | Live FNV conversation | new `docs/smoke-tests/m62-llm-dialogue.sh`: Doc Mitchell's house with `--extension <reference manifest> --extension-grant <id>=*`, `--bench-hold`; `byro-dbg`: activate Doc Mitchell, poll `dialogue.status`, `quest.show`, `inference.status` | FNV install, user-supplied model | Authored greeting first, then `provenance=generated generation=presented`; `quest.show` unchanged; `inference.status` shows one model, zero faults |
| 8 | Live Skyrim conversation | same script, `skyrim` argument, the `dt2` cell and NPC | Skyrim SE install, user-supplied model | As 7 |
| 9 | Frame cost | [ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench) on the scenario-7 cell, pinned camera, with and without a streaming request, same session | FNV install, user-supplied model | p95 delta within the Q6 threshold (LLMD-RES-005) |
| 10 | No headroom | `extensions::tests::gpu_placement_refused_over_headroom` | stub backend, declared footprint above a mocked budget | `no-headroom` or CPU placement per Q2 (LLMD-RES-001) |
| 11 | Save without the extension | `extensions::tests::save_with_dialogue_extension_loads_without_it` | stub backend | Load succeeds, state opaque, conversation authored-only (LLMD-SAV-003) |
| 12 | Script trigger | `extensions::tests::skse_mod_event_requests_a_generated_line` | stub backend, fixture Papyrus `SendModEvent` | Line presented at the speaker; no quest change (LLMD-SCR-001, -005) |

---

## 8. Metrics

The numbers live in ROADMAP: the M62 row in [Tier
9](../../ROADMAP.md#tier-9--better-than-bethesda-capability-stretch-post-tier-4-horizon)
for state, and one row per measured figure in [Repro commands for every bench
claim](../../ROADMAP.md#repro-commands-for-every-bench-claim).

| Metric | Definition | Granularity |
|---|---|---|
| First-text latency | dialogue event delivered (LLMD-HOST-001) → first generated character on the surface, p50 / p95 | per backend, model class, hardware |
| Line latency | dialogue event delivered → `generation=presented`, p50 / p95 | per backend, model class, hardware |
| Frame-time cost | p95 frame time with a streaming request − same-session control p95 | per bench scene, hardware |
| Fallback rate | requests ending `fallback` ÷ requests accepted | per reason code, per scenario run |
| GPU residency | model resident bytes, from the source LLMD-RES-002 settles | per model class |
| Scenario pass rate | §7.2 scenarios passing ÷ scenarios applicable | per game, per wave |

---

## 9. Delivery sequence

Depends on M47.0 and M43 (done, §1), on the sandbox's ticketed tasks (ECS-014)
and package content artifacts (ARC-002; model weights MUST use the one
content-artifact entry shape and manifest-version step that the
[Visual Scripting PRD](visual-scripting.md) also uses, VSCR-AST-008), neither
built at HEAD (§3.1 rows 7, 13; [Sandboxed Linked Mods
§19](../engine/sandboxed-linked-mods.md#19-delivery-sequence)), and, for Phase
2's non-native twin, on the Game UI Integration PRD's dialogue-surface phase.

**Phase 0 — Decisions, design and measurements (no engine code).** Resolve §10.
Write `docs/engine/llm-dialogue.md` owning the inference service and the
dialogue interfaces. Close every **Phase 0: verify** item: backend determinism
(LLMD-INF-010), off-allocator VRAM visibility (LLMD-RES-002), the `dt2` legs
and the FO4, Oblivion and Starfield conversation routes (§7.1). Measure, in a
standalone program on the dev machine beside a release engine bench, first-text
latency and tokens per second for each Q2 candidate on two model sizes, so Q6
rests on numbers. Fix the content-rating list (LLMD-CONT-003). Exit: the design
is checked in, every verify item is closed or filed, and the Q2 latency table
is checked in under `docs/audits/`.

**Phase 1 — Host interfaces and the stub backend.** The dialogue event,
read-only context, deferred line command and ticketed generation interfaces
(LLMD-HOST-*), manifest fields for eligibility, model artifacts, hash, licence
and rating (LLMD-OPT-002, LLMD-INF-002, -003, LLMD-LIC-001, -003), the
inference trait with the stub (LLMD-INF-008), and the safety tests. Exit: §7.2
scenarios 2, 3, 4, 5, 10 and 11 pass.

**Phase 2 — Presentation.** Provenance tag, thinking cue, streaming subtitle,
cancellation, `dialogue.status` provenance (LLMD-DLG-*), subtitle-only voice
(LLMD-VOX-001, -002), the lane switch and purge (LLMD-OPT-005, LLMD-SAV-004),
hide (LLMD-CONT-004). Exit: scenarios 1 and 6 (native page) pass.

**Phase 3 — First real backend and the reference mod.** The Q2 backend behind
the trait, headroom placement, CPU pool and load policy (LLMD-RES-001, -003,
-004, -007), `inference.status` (LLMD-DX-001), the reference dialogue extension
(LLMD-G-007). Exit: scenarios 7, 8 and 9 pass and the latency metrics are
published in ROADMAP.

**Phase 4 — Script triggers.** `ModEvent` and script-function routes
(LLMD-SCR-001, -002, -005), the FO4 `CustomEvent` bridge (LLMD-SCR-003), the
SCDA route (LLMD-SCR-004). Exit: scenario 12 passes, plus its FO4 `CustomEvent`
twin.

**Phase 5 — Later waves and options.** Waves 2 and 3 as their routes are gated;
synthetic voice if Q4 admits it (LLMD-VOX-003, -004); player text if Q3 admits
it (LLMD-HOST-003). Exit: scenario 7 passes per wave-2 game and the scenario
pass rate is published per game.

---

## 10. Decisions required

Each needs the project owner; the recommendation comes first. Rev 1's Q8
(conversation memory in saves) is dropped: ROADMAP's "lives entirely in
plugin space" already decides it, as LLMD-SAV-001 and LLMD-NG-006 record.

1. **Q1 — Where inference runs, given "lives entirely in plugin space".**
   Recommended: the extension owns everything dialogue-specific (persona,
   prompts, memory, eligibility, policy) and calls a generic, capability-gated
   engine inference service that loads the model its own package ships (§5.3);
   "plugin space" then means the sandbox principal, not where the matrix
   multiplications run, and the ROADMAP row is reworded to say so. Alternative
   A: inference inside the guest; consequence: `SandboxConfig` ceilings rise by
   orders of magnitude, the guest has no GPU, and generation runs far slower
   than native (§5.3). Alternative B: a local inference server over a loopback
   socket (the Mantella / CHIM shape); consequence: it contradicts "no network
   calls", needs a network grant (SEC-004), and adds a side process the engine
   cannot budget.
2. **Q2 — First inference backend.** ROADMAP names candle and mistral.rs;
   candle's README lists CPU, CUDA and WASM, mistral.rs's lists "CUDA …,
   Metal", neither names Vulkan, and llama.cpp's backend table has a "Vulkan"
   row ([candle](https://github.com/huggingface/candle),
   [mistral.rs](https://github.com/EricLBuehler/mistral.rs),
   [llama.cpp](https://github.com/ggml-org/llama.cpp)). Recommended: choose by
   Phase 0 measurement between a pure-Rust CPU backend (candle or mistral.rs)
   and llama.cpp's Vulkan backend through `crates/cxx-bridge` (a placeholder,
   §3.1 row 17), CUDA at most optional (LLMD-INF-009) so the feature is not
   NVIDIA-only on a Vulkan engine; this rewrites the ROADMAP row's backend. Alternative A: candle or mistral.rs with CUDA as
   the default GPU path; consequence: proprietary toolkit at build and run
   time, no GPU path on AMD or Intel. Alternative B: engine-written Vulkan
   compute kernels; consequence: exact budget accounting, at the cost of
   maintaining an inference runtime.
3. **Q3 — How the player speaks.** Recommended: the player keeps choosing
   authored topics and the eligible NPC adds generated lines; typed free text
   is a Phase 5 option and speech-to-text is out of this PRD. Alternative A:
   typed free text from Phase 2; consequence: a text field on the surface (a
   GUI PRD requirement), a new input path into the sandbox (LLMD-HOST-003), and
   player prompt injection becomes the norm. Alternative B: speech-to-text
   (Mantella's Whisper / Moonshine shape); consequence: microphone access as a
   sensitive capability and a second model resident.
4. **Q4 — Generated voice.** Recommended: no synthesis in this PRD; generated
   lines are subtitle-only, as unvoiced authored lines are today. Alternative:
   package-shipped text-to-speech voices under declared licences (LLMD-VOX-003,
   -004); consequence: a second model and its VRAM or CPU cost, and a licence
   check per voice (Piper's maintained code is GPL-3.0 and its voices carry
   per-voice licences, §5.10). Voice cloning of real people is excluded either
   way (LLMD-NG-004).
5. **Q5 — May a model ever choose an authored topic?** Recommended: no; only
   the player selects authored topics (LLMD-SAFE-004). Alternative: a "router"
   mode in which the model maps player free text (Q3) to one of the offered
   authored topics, which then runs exactly as a click would; consequence: a
   misclassification can advance a quest the player did not mean to, so it
   needs a confirmation step on the surface.
6. **Q6 — Budgets.** Recommended: set the frame-time threshold, first-text and
   line latency targets and the VRAM margin from Phase 0's measurements, per
   hardware class (Memory Budget's RT-minimum and dev cards). Alternative: fix
   them now (say, "no measurable p95 change, first text inside two seconds");
   consequence: a promise made before any backend is measured under the
   engine's load.
7. **Q7 — Which models the reference mod uses, and where its weights come
   from.** Recommended: an Apache-2.0 model (Mistral 7B, or a Qwen2.5 size
   other than 3B / 72B) read from a user-supplied path, so the repository ships
   no weights (LLMD-LIC-004), with Llama 3.x supported for any mod that meets
   its licence (LLMD-LIC-001, -002); third-party mods still ship weights with
   the mod as ROADMAP says. Alternative A: Llama 3.x as the reference, as
   ROADMAP names; consequence: the reference mod carries Meta's attribution and
   naming terms, and the owner must first rule on whether in-fiction game
   violence falls under the AUP's "incite or promote violence" clause (§5.6).
   Alternative B: the project hosts the reference mod's weights as a separate
   download; consequence: the project distributes weights and needs hosting
   outside the repository.

---

## 11. Document map

| Need | Go to |
|---|---|
| Activation conversations, INFO selection, line lifetime, voice | [dialogue-trees.md](../engine/dialogue-trees.md) |
| Which surface draws dialogue, and its controls | [game-ui-integration.md](game-ui-integration.md) |
| Sandbox threat model, capabilities, host service families | [sandboxed-linked-mods.md §10](../engine/sandboxed-linked-mods.md#10-capability-and-security-model), [§11.1](../engine/sandboxed-linked-mods.md#111-host-service-families) |
| Extension persistence rules | [sandboxed-linked-mods.md §13](../engine/sandboxed-linked-mods.md#13-persistent-state-and-saves), [save-load-roundtrip.md](../engine/save-load-roundtrip.md#engine-native-extension-state) |
| SDK events, scheduling and the service catalog | [sdk-v0.1-development-plan.md §6.5](../engine/sdk-v0.1-development-plan.md#65-events-and-scheduling) |
| Condition evaluator, quest stages, custom events | [scripting.md](../engine/scripting.md) |
| VRAM targets | [memory-budget.md](../engine/memory-budget.md#vram-rough-budget-rtx-4070-ti-typical-fnv-interior) |
| Native-plugin dispositions and preflight | [legacy-mod-compatibility.md](legacy-mod-compatibility.md) |
| Per-game engine tiers | [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail) |
| Live dialogue gates | [smoke-tests/README.md](../smoke-tests/README.md) |
| Bench methodology | [ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: HEAD inventory corrections (select_info visibility, page inputs, existing read interfaces, capability descriptors, cxx-bridge placeholder); Mantella / CHIM claims re-quoted (no "cloud by default"); licence table with every Llama 3 / 3.1 / 3.2, Qwen2.5, Mistral, runtime and Piper term quoted and linked, licence verify items closed; dialogue surface handed to the Game UI Integration PRD (GUI-PAGE-004); closed refusal / fallback reason codes; RFC 2119 wording; ROADMAP M62 wording deviations surfaced (§6 row 8, Q7); sandbox prerequisites named; Q8 dropped as already decided by ROADMAP; tightening; coordinator cross-PRD sync (sibling links and citations) | claude: revise |
