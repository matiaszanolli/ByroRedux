# Sandboxed Linked Mods — Requirements and Architecture

**Status:** Requirements baseline; first live runtime/manifest lifecycle slice implemented

**Scope:** Executable community mods linked to ByroRedux and to each other
without sharing native address space

**Related:** [Plugin Loading](plugin-loading.md),
[Scripting](scripting.md), [Save/Load Round-Trip](save-load-roundtrip.md),
[UI System](ui.md), [ECS](ecs.md)

## 1. Decision summary

ByroRedux will treat executable mods as **sandboxed components** that are
linked to engine and community services through typed, versioned interfaces.
"Linked" means that a component can import engine capabilities and services
exported by other mods. It does **not** mean that it shares process memory,
Rust references, C++ objects, ECS locks, raw function pointers, or executable
addresses with the host.

The initial execution target is the WebAssembly Component Model with WIT
interfaces and a Wasmtime-based host behind an engine-owned runtime
abstraction. This gives C and C++ a practical compilation path while keeping
the public ABI language-neutral. The choice does not make unrestricted WASI
part of the mod API: operating-system access remains capability-gated and is
absent by default.

Executable code is only one artifact in a mod package. External content and
the declarations that attach it to the engine remain separate:

```text
ModPackage
  ├── immutable content artifacts ──► content resolver ──► attachment plan
  ├── extension components ─────────► component linker ─► isolated instances
  └── manifest ─────────────────────► identity, dependencies, permissions,
                                      imports, exports, and resource budgets

ResolvedModSet
  ├── content attachments
  ├── typed service-link graph
  ├── granted capability set per mod
  ├── deterministic schedule
  └── complete provenance and diagnostics
```

No arbitrary Windows DLL, script-extender binary, address-library relocation,
trampoline, or machine-code patch is accepted into the sandbox. Source code
that depends on semantic extender services can be ported and recompiled;
code that depends on the original executable's layout must be redesigned or
implemented as an independently verified engine feature.

Normative terms **MUST**, **SHOULD**, and **MAY** have their RFC 2119 meanings.

## 2. Why this exists

Bethesda mod ecosystems commonly combine hundreds of record plugins, archive
overlays, scripts, UI movies, native extensions, and compatibility patches.
The useful features of script extenders are their services: lifecycle events,
script-native registration, messaging, serialization, tasks, UI integration,
and access to game concepts. Their most fragile feature is unrestricted
access to private executable memory.

ByroRedux needs the former without inheriting the latter. The design must:

- preserve ordinary content mods and unofficial patches as external,
  load-ordered content;
- let code mods participate deeply in gameplay without gaining ambient host
  authority;
- make cross-mod collaboration a supported contract instead of an accidental
  dependency on memory layouts;
- work across the Gamebryo and Creation lineage through feature-oriented
  interfaces rather than one hard-coded game ABI;
- scale by indexing and event subscription rather than scanning or ticking
  every enabled mod;
- keep failures attributable, containable, and recoverable.

## 3. Terminology

| Term | Meaning |
|---|---|
| **Mod package** | Distribution and identity unit. It may contain content, code, both, or neither. |
| **Legacy content plugin** | ESM, ESP, ESL, ESH, or another game-authored record container. It is data, not executable host code. |
| **Content artifact** | Immutable parsed or opaque external content such as records, PEX, NIF, SWF, textures, or localization. |
| **Attachment** | A declaration that makes an artifact participate in a target such as the record universe, virtual filesystem, script runtime, UI, or extension runtime. |
| **Extension component** | Sandboxed executable component supplied by a mod package. |
| **Game definition** | Cross-platform description of a game's dialects, rules, features, and attachment handlers. It contains no installation paths. |
| **Installation profile** | Local official-game paths and files. |
| **Mod profile** | Enabled packages, content ordering, service bindings, grants, and configuration selected by the user. |
| **Host interface** | Versioned engine service imported by a component. |
| **Mod service** | Versioned interface exported by one mod and imported by another. |
| **Link** | A resolved import-to-export relationship. |
| **Capability** | Explicit authority to perform a class of operation or access a particular resource. |
| **Resource handle** | Typed, unforgeable reference to a host-owned or brokered resource. |
| **Security principal** | Unit to which permissions, budgets, state, faults, and attribution belong; normally one mod package. |

"Plugin" is avoided for executable components in this document because it is
already overloaded by Bethesda record files and Redux's existing
`PluginManifest`.

## 4. Goals

- **G-001 — Safe deep integration.** Mods can observe events, query world
  state, submit mutations, add services, persist state, and integrate with UI
  without receiving native engine memory.
- **G-002 — Cross-game reuse.** A component can run unchanged on multiple
  games when its required feature interfaces are present.
- **G-003 — Community composition.** Mods can publish and consume
  namespaced, typed services without engine source changes.
- **G-004 — C and C++ portability.** The SDK provides a documented,
  tested C/C++ path in addition to Rust and other languages supported by the
  component toolchain.
- **G-005 — High mod counts.** No internal identity or scheduling design is
  based on Bethesda's load-slot ceilings or a small fixed mod count.
- **G-006 — Explainability.** Every capability, link, attachment, conflict,
  event delivery, state blob, and fault can be attributed to a stable mod ID.
- **G-007 — Evolution.** Host APIs and community services can evolve through
  explicit versions and adapters rather than executable-version offsets.
- **G-008 — Content/code independence.** Content can remain active when an
  optional code component is disabled, and code can be distributed without
  embedding the content it consumes.

## 5. Non-goals

- **NG-001.** Loading existing SKSE, F4SE, xNVSE, OBSE, or similar native DLL
  binaries unchanged.
- **NG-002.** Reproducing the original executable's object layouts, vtables,
  calling conventions, global variables, or hook addresses.
- **NG-003.** Granting filesystem, network, process, environment, device, or
  clock access merely because a component was compiled against WASI.
- **NG-004.** Making malicious gameplay changes impossible after the user has
  explicitly granted the capability to make those changes. The sandbox
  protects the host and other principals; permissions and diagnostics bound
  authorized gameplay effects.
- **NG-005.** Guaranteeing zero overhead at the component boundary. APIs are
  designed for batching and predictable cost instead.
- **NG-006.** Defining every future community interface in engine core.
- **NG-007.** Bundling or redistributing third-party patch content with the
  engine.
- **NG-008.** Enabling hot reload in the first production milestone.

## 6. Current Redux anchors

This design extends existing registry-oriented patterns rather than exposing
`World` directly:

- [`PluginManifest`](../../crates/plugin/src/manifest.rs) currently carries
  stable identity, name, version, and dependencies. It is not yet a package or
  executable-component manifest.
- [`DataStore`](../../crates/plugin/src/datastore.rs) stages candidate records
  and tracks conflict results. Its provenance model is relevant to mod-service
  and attachment resolution, although the live legacy load path currently
  merges `EsmIndex` values separately.
- [`ScriptRegistry`](../../crates/scripting/src/registry.rs) maps authored
  script identities to engine-owned behavior registration.
- [`SaveRegistry`](../../crates/save/src/registry.rs) is the curated boundary
  for durable ECS state.
- [`ScaleformHostBridge`](../../crates/ui/src/host.rs) demonstrates a semantic
  host bridge for legacy UI instead of exposing Scaleform pointers.
- [`Scheduler`](../../crates/core/src/ecs/scheduler.rs) already owns system
  ordering and declared access. Guest execution must preserve those
  invariants.
- [`GameProfileEntry`](../../crates/core/src/ecs/game_profiles.rs) describes
  installation paths and default official content. It must not become the
  executable mod ABI or the game-rules authority.
- [`parse_record_indexes_in_load_order`](../../byroredux/src/cell_loader/load_order.rs)
  currently reads files, assigns global legacy slots, parses, remaps, and
  merges in one operation. It identifies the seam between decoding and
  profile resolution.
- [`attach_vmad_scripts`](../../byroredux/src/cell_loader/references/attach.rs)
  currently finds external PEX bytes, translates them, and mutates the ECS in
  one operation. It identifies the seam between artifacts and attachments.

The workspace did not contain a WebAssembly runtime when this design was first
drafted. The first implementation now lives in
[`crates/mod-runtime`](../../crates/mod-runtime/) and uses Wasmtime behind the
engine-owned `SandboxRuntime` abstraction.

### 6.1 Implemented foundation (updated 2026-08-31)

The first vertical slice implements the narrow `byro:mod-host@0.1.0` WIT
world in [`host.wit`](../../crates/mod-runtime/wit/host.wit):

- one separately compiled and instantiated WebAssembly Component per
  principal/store;
- stable validated principal and capability identifiers;
- explicit, capability-gated, principal-attributed logging;
- read-only principal ID and effective-capability discovery;
- opaque generational entity references and manifest-ordered typed schemas;
- bounded callback-local entity projections with separately gated
  name/form-identity and world-transform visibility;
- canonical activation delivery gated by both declaration and capability;
- bounded own-component commands queued during callbacks and returned only
  after successful guest completion;
- principal-isolated dynamic rows with atomic batch application;
- component byte, per-memory, memory-count, table, instance, stack, fuel, and
  retained-log ceilings;
- `ready -> active -> stopped` lifecycle transitions and fault quarantine;
- no linked WASI implementation or ambient operating-system imports;
- a headless test harness covering allowed and denied calls, instance fault
  isolation, fuel exhaustion, memory rejection, log/command bounds, absent
  WASI, principal state isolation, and activation rollback after a trap.

This remains an early mod loader, not a complete extension platform. The SDK now
defines and validates the executable-extension subset of the package manifest,
and the plugin crate resolves those manifests through the same dependency graph
primitive used by record plugins. The runtime checks SDK compatibility before
compilation, checks requested/effective capabilities before instantiation, and
publishes version discovery through WIT. The executable now owns a live host:
repeatable `--extension` manifests resolve dependency-first, explicitly granted
capabilities are applied, the complete profile stages before one atomic swap,
live activation markers are delivered outside ECS guards, component state is
applied atomically, faults remain isolated, world replacement invalidates
transient handles, and orderly shutdown runs in reverse publication order.
The adapter snapshots disclosed entities before guest entry, and the
`byro.world` service rejects undisclosed or stale handles without exposing ECS
storage. Entity-attached extension rows and principal-private storage are
embedded in the checksummed engine save,
keyed by stable form identity rather than ECS IDs. Load preflight rejects
unsupported formats or active-schema mismatches before teardown; missing
packages and temporarily absent forms remain opaque and are written back
unchanged, while returning forms rebind before event delivery.
Immutable artifact hashing, compilation cache, broader read-only ECS
projections, linked community services, schema migration, status tooling,
C/C++ examples, and high-count scheduling remain in the later phases below. In
particular, the
current `StoreLimits` memory ceiling
applies to each linear memory; the separate memory-count ceiling also bounds
how many a component may create.

## 7. Architectural boundaries

### 7.1 Package, content, and attachment

- **ARC-001.** A mod package MUST have a stable `ModId` independent of file
  path, load order, installation order, and enabled state.
- **ARC-002.** A package manifest MUST declare content artifacts separately
  from their attachments.
- **ARC-003.** A package manifest MUST declare executable components
  separately from content artifacts and attachments.
- **ARC-004.** Parsing, linking, and attachment planning MUST complete without
  mutating the live ECS world.
- **ARC-005.** A resolved profile MUST be immutable once activated. A profile
  change creates a new generation and commits it atomically.
- **ARC-006.** External bytes MUST remain immutable and retain their source
  path, content hash, package identity, and attachment provenance.
- **ARC-007.** Disabling a component MUST NOT implicitly remove sibling
  content unless the package manifest declares that dependency or the user
  disables the whole package.
- **ARC-008.** Load order MAY affect resolution but MUST NOT define internal
  object, package, component, or saved-state identity.
- **ARC-009.** The core attachment and service registries MUST use namespaced
  identifiers rather than closed enums so extensions can add new targets and
  services.
- **ARC-010.** Unknown content MUST remain opaque and inspectable where safe;
  lack of a decoder MUST NOT silently erase it from provenance.
- **ARC-011.** Components MUST reference external content through declared
  artifact IDs, content IDs, virtual paths, or capability-scoped handles—not
  absolute installation paths.
- **ARC-012.** Components MUST NOT mutate content artifacts or the active
  resolution snapshot. Authored output goes to a new artifact/package;
  runtime state goes to the principal's private state namespace.

### 7.2 Game abstraction

- **GAME-001.** `GameDefinition`, `InstallationProfile`, and `ModProfile` MUST
  remain distinct types with distinct ownership.
- **GAME-002.** Components MUST target required feature interfaces and
  versions, not branch on a mandatory monolithic `GameKind` ABI.
- **GAME-003.** The host MUST expose a read-only game-feature query returning
  stable feature IDs, interface versions, and compatibility facts.
- **GAME-004.** Canonical interfaces SHOULD express shared concepts such as
  forms, actors, inventory, quests, events, UI, and time.
- **GAME-005.** Game-specific interfaces MAY expose authored quirks that
  cannot be represented honestly by a canonical contract.
- **GAME-006.** Legacy-compatibility facades MUST translate into the same host
  services used by native Redux components; they MUST NOT create a second
  privileged execution path.
- **GAME-007.** A single component binary MUST be able to activate on two game
  definitions when both satisfy its declared feature requirements.

## 8. Package manifest requirements

The eventual schema name is deliberately left open. It represents a
`ModManifest`, not the existing record-oriented `PluginManifest`.

- **PKG-001.** The manifest MUST declare `ModId`, human-readable name, SemVer
  version, and manifest-schema version.
- **PKG-002.** Dependencies MUST identify stable `ModId` values and version
  ranges. Display names are informational only.
- **PKG-003.** Every executable component MUST declare its component path,
  content hash, implemented WIT world, and security principal.
- **PKG-004.** Every requested host capability MUST be declared. A request is
  not a grant.
- **PKG-005.** Imports and exports MUST use globally namespaced interface IDs
  with versions.
- **PKG-006.** The manifest MUST distinguish required imports, optional
  services, and ordering-only relationships.
- **PKG-007.** The manifest MUST support multiple components in one package.
  Components in the same principal MAY be composed at package build time.
- **PKG-008.** Content paths MUST be package-relative. Absolute host paths and
  path traversal MUST be rejected before activation.
- **PKG-009.** Requested budgets and failure policy MAY be declared, but the
  host or user policy always sets the effective ceiling.
- **PKG-010.** The resolved manifest, component hashes, content hashes, grants,
  and selected providers MUST contribute to the profile fingerprint stored in
  saves and diagnostics.
- **PKG-011.** Package signatures MAY establish publisher trust, but signature
  presence MUST NOT silently grant capabilities.
- **PKG-012.** The manifest MUST allow content-only and code-only packages.
- **PKG-013.** Two packages claiming the same `ModId` and version with
  different content hashes MUST produce an identity-collision diagnostic and
  MUST NOT silently replace one another.

Illustrative structure, not a frozen TOML schema:

```toml
[mod]
id = "org.example.weather-overhaul"
version = "2.1.0"

[[artifacts]]
id = "records"
path = "content/WeatherOverhaul.esp"
format = "bethesda:tes4-plugin"

[[attachments]]
artifact = "records"
target = "byro:records"
policy = "profile-order"

[[components]]
id = "runtime"
path = "code/weather.wasm"
world = "org.example:weather/runtime@2.0.0"

[[imports]]
component = "runtime"
interface = "byro:world/weather@1"
required = true

[[exports]]
component = "runtime"
interface = "org.example:weather/control@2"

[capabilities]
request = [
  "byro.world.weather.read",
  "byro.world.weather.command",
  "byro.events.weather.subscribe",
]
```

## 9. Component and linking model

### 9.1 Binary and interface format

- **LINK-001.** The initial portable executable format MUST be a WebAssembly
  Component with machine-readable WIT imports and exports.
- **LINK-002.** The public mod ABI MUST be defined in WIT or an equivalent
  language-neutral schema, never Rust layout or C++ class layout.
- **LINK-003.** Component linear memories MUST NOT be imported, exported, or
  shared across security principals.
- **LINK-004.** The engine MUST hide the selected WebAssembly runtime behind a
  narrow internal abstraction so runtime upgrades do not become mod ABI
  changes.
- **LINK-005.** The exact runtime and code-generation versions MUST be pinned
  in reproducible builds and included in compiled-cache keys.

### 9.2 Link resolution

- **LINK-010.** Every required import MUST resolve before profile activation.
- **LINK-011.** An import may be satisfied by the host, an engine adapter, or
  a mod service whose export is type- and version-compatible.
- **LINK-012.** Provider selection MUST be deterministic and recorded. If two
  equally valid providers exist without an explicit rule, activation MUST
  report an ambiguity rather than silently choose one.
- **LINK-013.** Missing required imports MUST prevent activation of the
  component and its required dependents before the live profile changes.
- **LINK-014.** Optional services MUST be obtained through an explicit service
  discovery interface or a generated adapter world. They MUST NOT appear as
  silently missing required imports.
- **LINK-015.** Required static link dependencies MUST form a directed acyclic
  graph in the first implementation. Cycles MUST produce a path diagnostic.
- **LINK-016.** Runtime event exchange MAY be cyclic because it is queued and
  does not define construction order.
- **LINK-017.** Interface matching MUST account for version. Compatible
  adapters MAY satisfy older interfaces; an unreviewed major-version
  substitution MUST NOT.
- **LINK-018.** Community-defined interfaces MUST use publisher-controlled
  namespaces and MUST be linkable without adding variants to an engine enum.
- **LINK-019.** The resolved link graph MUST be inspectable before launch and
  while the profile is active.
- **LINK-020.** Linking MUST NOT union the caller's and provider's host grants.
  Each host call is checked against the principal that makes it.
- **LINK-021.** A service export MUST declare the host-effect capabilities it
  may exercise on a caller's behalf. The service link itself is explicit
  delegated authority to request only those effects; it does not transfer the
  provider's underlying host handles or unrelated grants.

### 9.3 Isolation topology

The default security principal is one package. Multiple components owned by
the same package may be pre-composed, but cross-package service links remain
host-observable.

- **LINK-030.** The implementation MUST isolate each security principal's
  memory, budgets, state, logs, handles, and fault status.
- **LINK-031.** Cross-principal service calls MUST pass through a broker or
  equivalent observable boundary that preserves caller and provider identity.
- **LINK-032.** Cross-principal calls MUST use WIT values or typed brokered
  resources, not guest pointers.
- **LINK-033.** Guest-owned resources MUST NOT cross principals until the
  broker has a defined proxy, ownership, lifetime, and failure model for that
  resource type.
- **LINK-034.** The host MUST impose a maximum synchronous cross-mod call depth
  and reject re-entrant cycles. Long or cyclic workflows use queued events or
  asynchronous task handles.
- **LINK-035.** A prototype MUST compare per-principal Wasmtime stores against
  safe composed link groups before store topology becomes a final ADR. The
  selected topology still has to satisfy LINK-030 through LINK-034.
- **LINK-036.** Nested service calls, host calls, emitted events, and submitted
  commands MUST retain the initiating call chain and the principal that
  actually performed each action.

## 10. Capability and security model

### 10.1 Threat model

The host treats both executable components and their external files as
untrusted input. Relevant threats include memory corruption attempts,
infinite computation, memory exhaustion, event or host-call floods, forged or
stale handles, path traversal, authority laundering, provider impersonation,
oversized results, malformed saved state, UI spoofing, and accidental faults
in otherwise trusted mods.

The sandbox does not eliminate vulnerabilities in the WebAssembly runtime or
host implementations. Runtime patch policy, strict host-call validation,
fuzzing, and defense in depth remain required.

### 10.2 Requirements

- **SEC-001.** Components MUST receive no ambient operating-system authority.
- **SEC-002.** Generic WASI filesystem, network, environment, process, clock,
  random, stdio, and device interfaces MUST be absent unless explicitly
  granted and virtualized.
- **SEC-003.** Filesystem grants MUST be preopened, path-confined capabilities.
  A private per-mod data directory SHOULD be preferred over arbitrary paths.
- **SEC-004.** Network access MUST be denied by default and require an explicit
  user-visible grant scoped by policy.
- **SEC-005.** Components MUST NOT receive `World*`, `EntityId` internals,
  renderer objects, allocator pointers, file descriptors, native callbacks,
  or other forgeable host references.
- **SEC-006.** Host resources MUST use typed, unforgeable, generational handles
  with principal, lifetime, and permission checks.
- **SEC-007.** Every host call MUST validate lengths, enum values, UTF-8/path
  rules, handle ownership, capability grants, result limits, and lifecycle
  phase before acting.
- **SEC-008.** A component MUST have configurable ceilings for linear memory,
  tables, instances, stack, host resources, output sizes, queued events,
  outstanding tasks, saved-state bytes, and log volume.
- **SEC-009.** Every guest entry must have bounded execution through fuel,
  epoch interruption, or a measured combination. Infinite guest execution
  MUST NOT stall the engine indefinitely.
- **SEC-010.** Resource and execution limits MUST apply to initialization,
  lifecycle hooks, service calls, event handlers, save/load hooks, and failure
  cleanup—not only per-frame callbacks.
- **SEC-011.** A trap, timeout, invalid handle, or quota violation MUST unwind
  to the host without terminating the engine process.
- **SEC-012.** A faulted principal MUST be quarantinable: cancel its tasks,
  stop event delivery, revoke handles, block new calls, and mark dependents
  without corrupting the active content snapshot.
- **SEC-013.** A component MUST NOT choose its own effective grants or raise
  its own quotas at runtime.
- **SEC-014.** Capability delegation to another mod MUST be explicit,
  attenuated, attributable, and revocable.
- **SEC-015.** Structured logs and UI text from mods MUST be tagged and safely
  rendered so they cannot impersonate engine diagnostics or inject terminal
  control behavior.
- **SEC-016.** Compiled component caches MUST be invalidated by component hash,
  runtime version, target, security-relevant configuration, and host-interface
  fingerprint.
- **SEC-017.** Development mode MAY expose stronger debugging facilities, but
  production profiles MUST NOT inherit those grants.
- **SEC-018.** Content-read capabilities MUST be scoped to declared package
  artifacts, resolved public mounts, or explicit shared resources. A
  component MUST NOT bypass virtual-path resolution to discover losing or
  private external files unless separately granted for diagnostics.

## 11. Engine and ECS integration

### 11.1 Host service families

The first host surface should be a set of small interfaces, not a god object:

```text
byro:host/log             structured, attributed logging
byro:host/metadata        package/profile/component facts
byro:game/features        game-definition capability query
byro:content/read         capability-scoped immutable content access
byro:records/read         resolved record and provenance snapshots
byro:world/query          batched ECS-derived snapshots
byro:world/commands       validated deferred mutations
byro:events               subscriptions and queued delivery
byro:tasks                bounded asynchronous work
byro:state                namespaced persistent state
byro:ui                   semantic UI registration and messages
byro:services             optional service discovery
byro:time                 engine/game time, when granted
byro:random               deterministic streams, when granted
```

- **ECS-001.** Guest components MUST observe world state through immutable
  snapshots, iterators owned by the host, or bounded query results.
- **ECS-002.** Guest mutations MUST be submitted as validated commands and
  applied at scheduler barriers.
- **ECS-003.** The host MUST NOT hold ECS component/resource locks while
  executing guest code.
- **ECS-004.** Guest code MUST NOT perform structural ECS mutation directly.
- **ECS-005.** Entity-facing handles MUST include generation or equivalent
  stale-reference protection. Durable object references use stable
  `(ModId, LocalObjectId)`-style keys rather than runtime entity IDs.
- **ECS-006.** Query and command interfaces MUST support batching so common
  operations do not require one boundary crossing per entity or field.
- **ECS-007.** Command validation MUST report partial failure at command
  granularity. Whether a batch is atomic MUST be explicit in the interface.
- **ECS-008.** Components MAY register event-driven systems with declared
  reads, writes, stage, priority, and frequency; unrestricted implicit
  per-frame execution MUST NOT be the default.
- **ECS-009.** The scheduler MUST dispatch only subscribed components for an
  event or phase.
- **ECS-010.** Independent guest jobs MAY run concurrently against immutable
  snapshots. Their command application order MUST remain deterministic.
- **ECS-011.** Ordering MUST be derived from explicit dependencies, stage,
  profile policy, and stable identity—not filesystem enumeration order.
- **ECS-012.** Host-originated events MUST carry stable schemas and an explicit
  delivery phase. Transient ECS markers may remain the internal producer.
- **ECS-013.** Mod-originated events MUST be namespaced, schema-versioned, and
  subject to queue and payload limits.
- **ECS-014.** Latent work MUST use task/ticket resources. A guest call MUST
  not block an engine thread waiting for arbitrary I/O or another mod.
- **ECS-015.** Canonical host services MUST enforce the same world invariants
  as engine-native systems.

## 12. Lifecycle and failure semantics

```text
discover → hash/verify → resolve content → resolve links/grants
         → compile/cache → instantiate → initialize → plan attachments
         → validate → atomically activate profile generation
         → events/services/tasks/save-load
         → deactivate → shutdown → release principal resources
```

- **LIFE-001.** Instantiation MUST have no implicit world mutation. The host
  calls an explicit initialization export after grants and links are final.
- **LIFE-002.** Lifecycle phases and legal host calls in each phase MUST be
  documented and machine-testable.
- **LIFE-003.** Initialization follows dependency order; shutdown follows the
  reverse order.
- **LIFE-004.** Attachments and component instances MUST be prepared and
  validated before the new profile generation replaces the active one.
- **LIFE-005.** Failure while preparing a profile MUST leave the old profile
  unchanged.
- **LIFE-006.** Runtime failure policy MUST distinguish disabling one
  component, disabling its dependent components, disabling the package, and
  aborting the session. The effective policy is visible to the user.
- **LIFE-007.** Content attachments MAY remain active after an optional
  component fault when the manifest dependency graph permits it.
- **LIFE-008.** The engine MUST prevent new inbound service calls to a
  quarantined provider and notify or quarantine required dependents.
- **LIFE-009.** Tasks and handles MUST be cancelled or invalidated when their
  owning principal deactivates.
- **LIFE-010.** Development hot reload, when added, MUST use the same
  prepare/validate/commit protocol and state migration rules as a profile
  generation change.

## 13. Persistent state and saves

- **STATE-001.** Each principal MUST receive a private, versioned state
  namespace keyed by stable `ModId`, not load order or component path.
- **STATE-002.** A component MUST declare its state schema/version and maximum
  exported size.
- **STATE-003.** Mod state MUST enter the save through an engine transaction;
  components MUST NOT write directly into the engine save file.
- **STATE-004.** Save hooks MUST have execution and size limits. One stalled
  component MUST not hang the save indefinitely.
- **STATE-005.** The save transaction MUST define whether a component failure
  aborts the save, preserves its last known-good blob, or omits the new blob.
  The outcome MUST be reported; silent truncation is forbidden.
- **STATE-006.** Load MUST provide the component with its prior version and
  state bytes so it can run a bounded migration before activation.
- **STATE-007.** Failed migration MUST not expose partially migrated state.
- **STATE-008.** Opaque state for a temporarily absent mod SHOULD be preserved
  within configured quotas so re-enabling the mod can recover it. Users MUST
  be able to inspect and purge orphan state.
- **STATE-009.** Durable references in mod state MUST use stable object keys or
  host serialization helpers. Runtime entity handles MUST be rejected or
  explicitly resolved during load.
- **STATE-010.** State restore order MUST follow the resolved service
  dependency graph before world-ready events are delivered.
- **STATE-011.** The profile fingerprint, selected service providers, grants,
  component versions, and state-schema versions MUST be stored with the save.
- **STATE-012.** Mod state MUST be validated independently so malformed state
  cannot corrupt unrelated engine or mod state.
- **STATE-013.** A principal MUST NOT read, replace, enumerate, or delete
  another principal's private state unless that state is exposed through an
  explicit mod service.

The existing [save registry](save-load-roundtrip.md) remains authoritative for
engine-owned ECS columns and resources. Sandboxed state is an opaque,
namespaced extension to that curated save contract, not arbitrary ECS
serialization.

## 14. Performance and scale

- **SCALE-001.** No API, ID, array index, scheduler field, or manifest format
  may impose a Bethesda-style small fixed mod count.
- **SCALE-002.** A conformance profile MUST exercise at least 1,000 enabled
  packages, including at least 500 executable principals, even if most are
  idle.
- **SCALE-003.** Runtime service lookup MUST be indexed by interface/provider;
  it MUST NOT scan all mods.
- **SCALE-004.** Event delivery cost MUST scale with matching subscribers, not
  total enabled components.
- **SCALE-005.** Idle components MUST consume no scheduled CPU merely because
  they are enabled.
- **SCALE-006.** Component compilation MUST be cached by immutable inputs.
  Profile changes SHOULD recompile only changed components or invalidated
  interfaces.
- **SCALE-007.** Content parsing, component compilation, and independent link
  validation SHOULD run in parallel before activation.
- **SCALE-008.** Each principal and the aggregate runtime MUST expose memory,
  fuel/time, host-call, event, task, and state telemetry.
- **SCALE-009.** Boundary APIs MUST provide batch forms for high-volume world,
  record, event, and command traffic.
- **SCALE-010.** Large payloads SHOULD use bounded streams or host resources
  rather than repeated copies into unbounded lists.
- **SCALE-011.** Activation and steady-state benchmarks MUST publish hardware,
  package/component counts, cache state, and percentile timings rather than a
  context-free single threshold.
- **SCALE-012.** The host MAY lazily instantiate components that have no
  startup responsibilities, required exports, or active subscriptions.
- **SCALE-013.** Resource exhaustion MUST degrade by rejecting or quarantining
  the responsible principal, not by violating other principals' budgets.

## 15. API evolution and compatibility

- **API-001.** Host and mod-service interfaces MUST be individually versioned.
- **API-002.** Breaking changes require a new major interface version.
- **API-003.** Multiple major versions MAY coexist when resource budgets
  permit.
- **API-004.** Adapter components MAY translate between interface versions and
  MUST appear explicitly in the link graph.
- **API-005.** Capability names and interface versions MUST be separate:
  implementing an interface does not automatically grant every operation it
  describes.
- **API-006.** Deprecated interfaces MUST have documented replacements and
  diagnostics before removal.
- **API-007.** The engine MUST publish conformance tests and generated bindings
  for every stable host-interface version.
- **API-008.** The engine SHOULD prefer small orthogonal interfaces over one
  versioned monolith so mods depend only on the semantics they use.
- **API-009.** Legacy extender compatibility MUST be documented per semantic
  service or script function, not as a misleading global compatibility flag.
- **API-010.** Address-library, trampoline, raw-memory, and executable-patch
  APIs MUST remain unavailable even under a broad gameplay capability grant.

## 16. C and C++ SDK requirements

The C/C++ SDK is a source-porting target, not a compatibility claim for native
DLLs.

- **CPP-001.** CI MUST build and run at least one C and one C++ component
  against the same WIT host contract used by Rust examples.
- **CPP-002.** Generated bindings MUST hide canonical-ABI allocation details
  behind documented ownership functions or RAII wrappers.
- **CPP-003.** The SDK MUST document supported language level, standard-library
  surface, exception configuration, threading model, dynamic-linking limits,
  and unavailable platform APIs.
- **CPP-004.** SDK examples MUST avoid native pointers in public interfaces and
  demonstrate typed handles, batched commands, events, state migration, and
  error handling.
- **CPP-005.** C++ exceptions MUST NOT cross the component boundary. Guest
  failures cross as typed results or traps.
- **CPP-006.** Components MUST NOT spawn unmanaged native threads. Any guest
  concurrency uses supported WebAssembly/WASI facilities or `byro:tasks`.
- **CPP-007.** Static libraries MAY be linked into a component when their
  license and WebAssembly portability permit it; arbitrary native dynamic
  libraries cannot be loaded by path.
- **CPP-008.** The SDK SHOULD include a migration guide mapping common
  script-extender concepts—messaging, serialization, task dispatch, Papyrus
  registration, UI hooks—to Redux host services.
- **CPP-009.** Toolchain and generated-binding versions MUST be reproducibly
  pinned in the example templates.

## 17. Diagnostics and developer experience

- **DX-001.** A headless mod host MUST allow components to be tested without a
  renderer or installed commercial game.
- **DX-002.** The SDK MUST provide mock or recorded implementations of stable
  host interfaces.
- **DX-003.** Profile tooling MUST explain content winners, attachments,
  imports, selected providers, adapters, grants, budgets, and disabled nodes.
- **DX-004.** Runtime tooling MUST report per-principal calls, traps, time/fuel,
  memory, queue depth, task count, state size, and recent diagnostics.
- **DX-005.** Logs, traces, events, commands, and service calls MUST carry
  principal and component identity.
- **DX-006.** Component backtraces and source maps SHOULD be retained in a
  user-selectable development profile without weakening production grants.
- **DX-007.** A deterministic event/call recording format SHOULD support
  reproduction of failures in the headless host.
- **DX-008.** The debug CLI SHOULD eventually expose equivalents of:

  ```text
  mod.list
  mod.graph
  mod.inspect <mod-id>
  mod.links <mod-id>
  mod.grants <mod-id>
  mod.stats <mod-id>
  mod.faults <mod-id>
  mod.explain-path <virtual-path>
  mod.explain-object <object-key>
  ```

- **DX-009.** Link and capability errors MUST identify the requesting mod,
  interface/capability, requested version, candidate providers, and rejection
  reason.
- **DX-010.** A package validator MUST run without activating executable code.

## 18. Required verification scenarios

The first production-capable implementation is incomplete until automated
tests cover all of the following:

1. **Sandbox escape surface:** a component cannot access filesystem, network,
   environment, clock, process, or host memory without the matching grant.
2. **Execution containment:** infinite loop, deep recursion, memory growth,
   oversized result, log flood, event flood, and host-call flood are bounded.
3. **Handle safety:** random, stale, cross-principal, and use-after-revoke
   handles are rejected without host panic.
4. **Typed linking:** required host import, required mod-service import,
   optional service, compatible adapter, missing provider, ambiguous provider,
   major-version mismatch, and dependency cycle produce deterministic results.
5. **Authority preservation:** a caller cannot acquire the provider's host
   handles or cause effects beyond the service's declared delegated authority.
6. **Fault propagation:** a provider trap quarantines the provider, prevents
   new calls, reports required dependents, and leaves unrelated mods running.
7. **ECS safety:** guest code runs without ECS locks; invalid commands are
   rejected; independent command buffers commit in deterministic order.
8. **Content/code separation:** disabling an optional component leaves its
   independent record and asset attachments unchanged; the component cannot
   bypass resolved content visibility or mutate those artifacts.
9. **Cross-game binary:** one component binary activates against at least two
   game definitions using only shared feature interfaces.
10. **C++ component:** a C++ example imports host services, subscribes to an
    event, submits a batched command, exports a service, and round-trips state.
11. **Persistence:** state migration succeeds atomically; corrupt, oversized,
    absent-mod, and faulting save states follow the documented policies.
12. **Scale:** a synthetic profile with 1,000 packages and 500 executable
    principals resolves, links, activates, dispatches targeted events, saves,
    loads, and shuts down with attributable resource telemetry.
13. **Profile transaction:** failure during preparation leaves the previous
    active generation untouched.
14. **Provenance:** tooling can explain an object's content winner, attachment,
    component commands, service provider, and grants from one trace.

## 19. Delivery sequence

### Phase 0 — contracts and spikes

- Freeze terminology and requirement IDs.
- Define a minimal WIT package for logging, metadata, lifecycle, and feature
  discovery.
- Prototype C and C++ component builds.
- Resolve the per-principal store versus composed-link-group question.
- Measure call, value-copy, instantiation, and compilation-cache costs.

### Phase 1 — isolated host

- Add the runtime abstraction and selected Wasmtime implementation.
- Load one component from immutable package content.
- Enforce grants, memory/resource limits, interruption, logging, and fault
  quarantine.
- Add the headless harness and package validator.

### Phase 2 — ECS attachment

- Add game-feature discovery, events, immutable queries, typed handles,
  batched commands, and scheduler barriers. Activation, cell-load, and
  producer-resolved combat-hit delivery are implemented; equip and
  recurring-update adapters remain.
- Connect component activation to `ResolvedModSet` generation commits.
- Prove the same component on two game definitions.

### Phase 3 — linked community services

- Resolve versioned mod imports/exports.
- Add provider selection, adapters, broker attribution, dependency failure,
  and graph diagnostics.
- Prove cross-mod calls without shared memory or authority laundering.

### Phase 4 — durable integration

- Add namespaced save state, migrations, absent-mod preservation, tasks, UI,
  and performance telemetry.
- Run the full high-count conformance profile.

### Phase 5 — extender compatibility facade

- Map common extender messaging, serialization, task, UI, and script-native
  concepts onto stable Redux interfaces.
- Publish the C++ porting guide and compatibility matrix.
- Port representative source-available extensions; do not load their original
  DLL binaries.

## 20. Open design questions

These require prototypes or ADRs before implementation is considered stable:

1. **Store topology:** one Wasmtime store per principal gives clear isolation;
   direct component composition can simplify arbitrary WIT links. The chosen
   topology must preserve per-principal attribution and limits.
2. **Cross-store resources:** which resource types can be proxied safely and
   how ownership/destruction propagates when a provider faults.
3. **Asynchrony:** which Component Model async facilities are sufficiently
   stable for the first runtime and what remains expressed as host task/event
   handles.
4. **Determinism:** default policies for game time, monotonic time, random
   streams, parallel jobs, floating-point behavior, and replay.
5. **Grant UX:** how mod managers present capability diffs, publisher trust,
   profile inheritance, and headless-server policy.
6. **Budget defaults:** empirical memory, fuel/epoch, queue, host-call, and
   state limits for small, normal, and trusted profiles.
7. **Community interface distribution:** discovery and caching of WIT packages
   without making one centralized registry mandatory.
8. **Failure policy:** default behavior when executable code is required by
   otherwise valid content attachments.

## 21. Primary technical references

- [WebAssembly Component Model: components](https://component-model.bytecodealliance.org/design/components.html)
  — typed imports/exports, composition, and non-shared component memories.
- [WIT worlds](https://component-model.bytecodealliance.org/design/worlds.html)
  — explicit imported and exported interfaces as the component boundary.
- [Component composition](https://component-model.bytecodealliance.org/composing-and-distributing/composing.html)
  — satisfying imports from dependency-component exports.
- [WASI capability model](https://github.com/WebAssembly/WASI/blob/main/docs/Capabilities.md)
  — link-time capabilities and runtime resource handles.
- [Wasmtime security](https://docs.wasmtime.dev/security.html) — memory
  isolation, checked control flow, explicit imports, and filesystem
  capabilities.
- [Wasmtime component plugin example](https://docs.wasmtime.dev/wasip2-plugins.html)
  — dynamically loaded components, WIT bindings, and per-plugin stores.
- [Wasmtime resource limits](https://docs.wasmtime.dev/api/wasmtime/struct.StoreLimitsBuilder.html)
  — linear-memory, table, and instance ceilings.
- [Wasmtime interruption](https://docs.wasmtime.dev/examples-interrupting-wasm.html)
  — deterministic fuel and epoch-based interruption.
- [WASI SDK](https://github.com/WebAssembly/wasi-sdk) — Clang/LLVM C/C++
  compilation path and current exception, threading, dynamic-linking, network,
  and memory-model limitations.
