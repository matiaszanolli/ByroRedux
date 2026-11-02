# Decentralized Mod Hosting — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in
review round 1 (third-party server dependence of every transport,
discovery and trust option against the "Hosted online services"
anti-scope; `PluginId` stated precisely against the keep list; Q1 cites
ARC-001 instead of re-deciding it; the Construction Set and Skyrim CK
EULAs cited, the rest left to Phase 0; imported address lists can never
resolve a name; uncited BitTorrent, IPFS and BLAKE3 claims cited or
corrected). Statements about how an external format, protocol, tool or
mod-hosting ecosystem behaves cite external sources (the IPFS,
multiformats and UUID specifications, the BitTorrent BEPs, the minisign
and Sigstore documentation, the Nexus Mods file guidelines, the Creation
Kit EULAs and wiki); a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Scope**: M72 promises immutable, content-addressed *releases* of mod
packages; fetching a release and its dependency closure from any source
(§3.4) with every byte verified before it is stored, trusted or executed;
and profiles that pin releases by address, so they activate offline and
reproduce on another machine, with no central registry, index, updater or
account. This document owns
distribution, fetch, verification, the local store and pinning. Mod
identity, the mod profile, profile order and activation are owned by the
[Legacy Mod Compatibility PRD](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)
and are cited, never redefined. Outside it: search and browsing, ratings,
payments, automatic updates, Nexus or Bethesda.net integration, and any
redistribution of third-party or game-derived content (§4.2).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Mod package, stable `ModId`, package manifest, content hashes, signatures-don't-grant rule | [Sandboxed Linked Mods §7–8](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements) |
| Executable-code security: sandbox, capabilities, grants, component hash cache | [Sandboxed Linked Mods §10](../engine/sandboxed-linked-mods.md#102-requirements) |
| Mod profile, plugin identity, profile order, ownership, fingerprint, missing masters | [Legacy Mod Compatibility PRD §5.1](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id) |
| `PluginManifest`, `DependencyResolver`, Form ID layers | [Plugin Loading](../engine/plugin-loading.md#dependencyresolver--conflict-resolution) |
| Redux-native plugin output written by the editor | [In-Engine World Editor PRD §5.6](world-editor.md#56-output-plugin-wed-out) |
| Front end, boot contract, the deferred mods screen | [Launcher §7](../engine/launcher.md#7-mods--deferred-but-not-designed-away) |
| Release manifest schema, canonical encoding, store layout, feed record, the fetcher | none yet: **Phase 0 writes `docs/engine/mod-distribution.md`** |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `DMH-` prefix.

---

## 1. Product thesis

ROADMAP's Tier 10 entry for M72 promises "IPFS-style content-addressed
mod distribution … mods are addressable globally, no central marketplace,
no Bethesda.net-style platform tax"
([ROADMAP Tier 10](../../ROADMAP.md#tier-10--ecosystem-unlock-post-tier-4-horizon)).
The anti-scope entry "Hosted online services" draws the boundary: "No
telemetry, no updater, no crash reporter posting upstream, no central
server, no skin / monetization surface, no Bethesda.net-style mod
marketplace", with M72 in scope as "content-addressed, no central
registry" ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
Every transport, discovery and trust option below is classified against
that line in §3.4.

Who it is for:

1. **Players** who want a profile that works the same tomorrow and on
   another machine. Today a mod set is a folder of downloads and a
   remembered list; nothing pins which bytes were in it.
2. **Authors** who want to publish without a platform account. The
   first-party route is central: the Fallout 4 Creation Kit builds the
   archives and sends "the package uploaded to Bethesda.net"
   ([FO4 CK: Sharing Mods](https://falloutck.uesp.net/wiki/Sharing_Mods)).
   Third-party sites are central too, and their rules defer to the author:
   "Only the original author of a given file or asset can specify its
   distribution permissions"
   ([Nexus Mods: File Submission Guidelines](https://help.nexusmods.com/article/28-file-submission-guidelines)).
3. **The engine itself.** Executable mods are WebAssembly components run
   in a sandbox ([Sandboxed Linked Mods](../engine/sandboxed-linked-mods.md)).
   Fetching executable code from strangers is a supply-chain problem; it
   is only acceptable if what runs is exactly what the user pinned and
   granted.

The corollary that shapes every requirement below: **identity is stable;
releases are content-addressed; nothing is trusted because of where it
came from.** A mod keeps one `ModId` across every version; each version is
a release whose address is the hash of its manifest. A source is a hint:
bytes from a mirror, a gateway or a swarm are accepted only when they hash
to what the user pinned, and executable code runs only with the grants the
user gave that release. The engine never searches and never phones home:
a user finds a mod the way they do today, by a link someone published, and
the engine's job starts at the address.

---

## 2. Terminology

Terms owned elsewhere are linked; the rest are defined here once.

| Term | Meaning |
|---|---|
| **Mod package** | The distribution and identity unit; content, code, both or neither ([sandbox §3](../engine/sandboxed-linked-mods.md#3-terminology)). |
| **`ModId`** | A package's stable identity, "independent of file path, load order, installation order, and enabled state" ([ARC-001](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment)). At HEAD record plugins carry a `PluginId` and executable packages an `ExtensionId`; the unified manifest is not built (§3.2). |
| **Package manifest** | The manifest the package already carries: `PluginManifest`, `ExtensionManifest`, or the sandbox design's planned *ModManifest* ([§8](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements)). This PRD adds no third kind. |
| **Release** | One immutable version of a mod package: its package manifest, its artifacts and its release manifest. |
| **Release manifest** | The file this PRD introduces: `ModId`, version, every artifact with its path, length and digest, the dependency edges, the redistribution declaration, and optional signatures. Schema owned by the Phase 0 design. |
| **Digest** | A self-describing hash of a byte string, written as a multihash: hash-function code, digest length, digest value ([multiformats: multihash](https://github.com/multiformats/multihash)). The function is decision Q2. |
| **Release address** | The digest of a release manifest's canonical bytes. Changes with any byte of the release; never changes for the same bytes. |
| **Source** | A place the fetcher may try for bytes: a local path, an HTTPS URL, an IPFS CID behind a trustless gateway, a BitTorrent magnet. Never part of identity, never trusted for integrity. |
| **Endpoint** | A host the fetcher opens a connection to. Every source except a local path implies one or more; §3.4 lists which, and who operates them. |
| **Fetcher** | The user-invoked program that resolves, downloads, verifies and stores releases. Not the engine binary (DMH-FET-001); where it lives is decision Q6. |
| **Store** | The local directory of verified blobs, keyed by digest. |
| **Release lock** | The list of release addresses a mod profile pins, with the resolved dependency closure. Carried by the mod profile; contributes to its fingerprint ([LMC-ID-007](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)). |
| **Local pin** | A release manifest the fetcher writes for content the user already has (a download from a mod site). Never published, never served. |
| **Publisher key** | An Ed25519 key pair whose signature over a release address attests who published it (decision Q4). |
| **Feed** | A publisher-signed, sequenced, expiring list of release addresses for that publisher's packages, followed by key, in the manner of an IPNS record (§3.3). |
| **Redistribution declaration** | The author's statement, inside the signed release manifest, of whether third parties may serve the release's bytes. |

---

## 3. Core model

### 3.1 Identity layers

Four identities already exist or are already decided; this PRD adds the
fifth and sixth and changes none of the others.

| Layer | Identity | Stable across a new version | Owner |
|---|---|---|---|
| Record | `FormIdPair` (`PluginId`, `LocalFormId`) | yes | [Plugin Loading § Form ID System](../engine/plugin-loading.md#form-id-system) |
| Plugin | `PluginId`: a UUID v5 of the legacy filename, or the UUID a Redux-native manifest declares (§3.2 row 9); case-folding the filename is [LMC-ID-001](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)'s requirement, not HEAD | yes, while the filename or declared UUID is unchanged | [Plugin Loading § PluginId](../engine/plugin-loading.md#pluginid), LMC-ID-001 |
| Package | `ModId` | yes | [ARC-001](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment) |
| Profile | profile fingerprint | no (changes with content) | [LMC-ID-007](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id), [PKG-010](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements) |
| Release | release address | no | this PRD |
| Artifact | artifact digest | no | this PRD |

Two ROADMAP phrases say "content-addressed" about identities that are not
hashes of content. The keep list's "Plugin identity — Content-addressed
Form IDs" ([keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation))
describes a `PluginId` that is a hash of a *name* or a declared UUID, never
of plugin bytes (§3.2 row 9); what it keeps, identity independent of load
order, is true and this PRD changes none of it. The M72 row's "mod-id =
content hash" is settled by the sandbox design: ARC-001 requires a stable
`ModId` "independent of file path, load order, installation order, and
enabled state", and PKG-013's diagnostic for "Two packages claiming the
same `ModId` and version with different content hashes" presupposes that
identity and content hash are distinct
([§8](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements)).
This PRD therefore content-addresses the *release*, not the mod, as the
World Editor PRD does for its output plugin
([WED §3.4](world-editor.md#34-which-identity-the-output-plugin-follows)).
What remains for the owner is only the ROADMAP wording (decision Q1).

### 3.2 What exists at HEAD

The inventory below is the state at HEAD on 2026-11-01. It is
hand-written; Phase 1 replaces the rows it touches with the tests named in
§7.2.

| # | Item | Today |
|---|---|---|
| 1 | Record-plugin manifest | `PluginManifest` carries a UUID `id`, `name`, semver `version` and `dependencies: Vec<PluginId>`; `RawDependency` (`crates/plugin/src/manifest.rs:71`) keeps only `uuid`, so a dependency has no version range and a TOML `name` is ignored. No digest. |
| 2 | Executable-package manifest | `ExtensionManifest` (`crates/sdk/src/manifest.rs`) has `id: ExtensionId`, `version`, and `ExtensionDependency { id, version: VersionReq, optional }`. `ExecutableComponent` declares `id`, `path`, `world`, `world_version` and no content hash, although [PKG-003](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements) requires one. |
| 3 | Unified *ModManifest* | Not built; the sandbox design says so ([§8](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements)) and lists "immutable artifact hashing" as later-phase work ([§6.1](../engine/sandboxed-linked-mods.md#61-implemented-foundation-updated-2026-08-31)). |
| 4 | Dependency graph | `DependencyGraph<K>` in `crates/plugin/src/resolver.rs` provides `transitive_deps` and `dependency_order` (Kahn's algorithm, deterministic cycle path at line 170). It is shared by `DependencyResolver` and `ResolvedExtensionSet::resolve` (`crates/plugin/src/extension.rs:78`), and is `pub(crate)`: no other crate can call it. |
| 5 | Record conflict resolution | `DependencyResolver::resolve_winner` lets a dependent override its dependency and breaks independent ties by lowest `PluginId`. No production caller: the crate doc (`crates/plugin/src/lib.rs`) and #5609 say so, and `rg` finds no use of `DependencyResolver` outside `crates/plugin`. |
| 6 | Package loading | `load_requested_extensions` (`byroredux/src/extensions/install.rs:705`) reads `--extension <manifest.toml>` from local paths only; `read_package_artifact` (line 820) rejects a path that escapes the package directory and caps each component's size; grants come only from `--extension-grant`. |
| 7 | Network | No HTTP client crate (`reqwest`, `ureq`, `hyper`) is in `Cargo.lock`; the only `std::net` users in tracked Rust sources are the debug protocol's (`crates/debug-server/src/listener.rs:173`, bound to `127.0.0.1`, and `tools/byro-dbg/src/{main,tui}.rs`). `sha2` appears in `Cargo.lock` only as a dependency of `cranelift-codegen`, `wasmtime-environ`, `termwiz` and `wezterm-blob-leases`; no signature crate is present. |
| 8 | Launcher | `BootRequest.mods` is `ModsSpec { load_order }`, "Reserved for P5" (`crates/boot-request/src/lib.rs:174`). The launcher design promises that P1–P4 "make zero network requests" and puts a mod browser out of scope ([§11](../engine/launcher.md#11-out-of-scope)). |
| 9 | `PluginId` | The doc comment (`crates/core/src/form_id.rs:52`) says "Content-addressed plugin identifier". `PluginId::from_filename` (line 98) is `Uuid::new_v5(&PLUGIN_NAMESPACE, name.as_bytes())` over the name as given, with a fixed namespace (lines 31–38); a UUID v5 is "an SHA-1 hash … over a given Namespace ID value concatenated with the desired name value" ([RFC 9562 §5.5](https://www.rfc-editor.org/rfc/rfc9562#section-5.5)), so the identity hashes the *name*, not the content. `PluginId::from_uuid` (line 104) wraps the UUID a `plugin.toml` declares (`crates/plugin/src/manifest.rs:41`). Neither changes when the plugin's bytes change. |

### 3.3 Release trust states

Every release the fetcher reports is in exactly one state.

| State | Meaning | Reported as |
|---|---|---|
| **Mismatch** | Some byte did not hash to its declared digest, or the release manifest's identity disagrees with its package manifest. Nothing is stored. | "rejected: <artifact> digest mismatch from <source>" |
| **Verified, unsigned** | Every digest matches; no signature. Local pins are always in this state. | "verified, unsigned" |
| **Verified, new key** | Digests match; signed by a key not yet pinned for this `ModId`. | "verified, signed by <key id> (first use)" |
| **Verified, trusted** | Digests match; signed by the key pinned for this `ModId`. | "verified, signed by <key id>" |
| **Key changed** | Digests match; signed by a key other than the pinned one, with no rotation statement from the pinned key. | "publisher key changed: <old> → <new>; confirm" |

Feeds follow the IPNS record model: the name is derived from the public
key, each record is signed, carries a sequence number that "Represents the
current version of the record (starts at 0)" and a validity after which
it "becomes invalid" ([IPNS Record spec](https://specs.ipfs.tech/ipns/ipns-record/)).
Whether feeds are carried over IPNS itself or as a signed file on any
source is the Phase 0 design's choice; the observable rules are
DMH-DIS-003 and DMH-DIS-004.

### 3.4 Which options depend on a third-party server

The anti-scope rules out a *central* server, not every remote host: a
mirror or a gateway is someone's server. The line this PRD holds is that
no endpoint is compiled in or defaulted (DMH-FET-007), none is trusted for
integrity (DMH-FET-003), and every one is printed (DMH-FET-008). Each
option, classified:

| Option | Endpoint it implies | Who names it | Trusted for | Status here |
|---|---|---|---|---|
| Local path | none | user | nothing | Phase 1 |
| HTTPS mirror | the mirror's operator | user configuration or a release the user named | availability only; sees which files are requested | Phase 3 (Q3) |
| IPFS trustless gateway | the gateway's operator; the spec lets a client "verify its integrity without delegating any trust to the gateway itself" ([Trustless Gateway](https://specs.ipfs.tech/http-gateways/trustless-gateway/)) | user configuration | availability only; sees which CIDs are requested | Phase 3 (Q3) |
| Embedded IPFS node | bootstrap peers: "IPFS comes with a default list of trusted peers" ([IPFS: bootstrap list](https://docs.ipfs.tech/how-to/modify-bootstrap-list/)) | a compiled-in default unless replaced | peer discovery | not recommended (Q3 alternative A) |
| IPNS resolution of a feed | the DHT or a gateway, as above | Phase 0 design | nothing; records are signed (§3.3) | Phase 0 design picks the carrier |
| BitTorrent tracker | the tracker's operator ([BEP 3](https://www.bittorrent.org/beps/bep_0003.html) lists "A BitTorrent tracker" among a swarm's entities) | the magnet or torrent the user named, or configuration | peer lists only; sees the info-hash and the fetcher's address | Phase 4 (Q3), DMH-FET-010 |
| BitTorrent DHT | bootstrap nodes; a client with an empty routing table "will need the contacts included in the torrent file" ([BEP 5](https://www.bittorrent.org/beps/bep_0005.html)) | the torrent's `nodes` key or configuration | peer lists only | Phase 4 (Q3), DMH-FET-010 |
| BitTorrent peers | every peer in the swarm; "the downloaders upload to each other" (BEP 3) | the swarm | nothing; pieces are hash-checked ([BEP 52](https://www.bittorrent.org/beps/bep_0052.html)) | Phase 4, serving gated by Q7 |
| minisign publisher keys | none | the release | attribution only (DMH-TRU-004) | Phase 3 (Q4) |
| Sigstore keyless signing | Fulcio and Rekor, a "public-good, non-profit service" ([Sigstore](https://docs.sigstore.dev/about/overview/)) | compiled in by design | the trust path itself | excluded (Q4 alternative A) |
| Signed feed file | whatever source carries it | user | nothing; signature and sequence checked (DMH-DIS-003) | Phase 3 |
| Imported address list | whatever source carries it, read once | user, in that run | nothing (DMH-DIS-005) | Phase 3 |

Only Sigstore and an embedded IPFS node with its default bootstrap list put
a party the user did not name in the path; both are alternatives this PRD
recommends against.

---

## 4. Goals and non-goals

### 4.1 Goals

- **DMH-G-001 — Exact reproduction.** A release lock copied to another
  machine activates the same bytes: identical store digests, identical
  profile fingerprint.
- **DMH-G-002 — Nothing unverified is used.** Every corrupted or
  substituted byte in the tamper suite (§7.2) is rejected before it
  reaches the store.
- **DMH-G-003 — Offline by default.** A profile whose lock is satisfied by
  the store activates with no network at all, and a default-configured
  fetcher contacts zero endpoints.
- **DMH-G-004 — No central party.** No endpoint, index, key or account is
  compiled into the engine or the fetcher; every endpoint contacted comes
  from the user's configuration or from a release or feed the user asked
  for.
- **DMH-G-005 — Closure without a sorter.** Requesting one release yields
  its full dependency closure, deterministically, through the existing
  plugin DAG, and never reorders plugins beyond what LMC-ID-003 already
  does.
- **DMH-G-006 — Fetched code is no more trusted than local code.** A
  fetched executable component runs only with grants the user gave that
  release, and an update shows every capability it adds.
- **DMH-G-007 — Legal by construction.** The fetcher serves only releases
  whose author declared third-party redistribution, and publishing refuses
  bytes identical to official game content.

### 4.2 Non-goals

- **DMH-NG-001.** A central registry, index, default gateway, tracker,
  bootstrap list, address list or package namespace authority (ROADMAP
  anti-scope "Hosted online services": "no central server"; §3.4).
- **DMH-NG-002.** Search, browsing, ratings, comments or download counts
  inside the engine, the launcher or the fetcher. Discovery stays where it
  is today, outside the engine (decision Q8).
- **DMH-NG-003.** Payments, prices, donations or any monetization surface
  (ROADMAP anti-scope "no skin / monetization surface"; the Skyrim SE
  Creation Kit EULA states that "All Game Mods developed or created by You
  using the Editor are for non-commercial use",
  [UESP: Skyrim Mod:EULA](https://en.uesp.net/wiki/Skyrim_Mod:EULA)).
- **DMH-NG-004.** An updater: background checks, scheduled feed polls,
  self-update. The fetcher acts only when the user runs it (ROADMAP
  anti-scope "no updater").
- **DMH-NG-005.** Telemetry of any kind, including download or install
  reporting (ROADMAP anti-scope).
- **DMH-NG-006.** Redistributing third-party mods without the author's
  declaration, or anything derived from game data
  ([LMC-NG-005](legacy-mod-compatibility.md#42-non-goals),
  [NG-007](../engine/sandboxed-linked-mods.md#5-non-goals)).
- **DMH-NG-007.** A load-order sorter or masterlist
  ([LMC-NG-004](legacy-mod-compatibility.md#42-non-goals), ROADMAP
  anti-scope "no LOOT-equivalent"). Closure decides *which* releases are
  present, never their order.
- **DMH-NG-008.** Nexus Mods API, Bethesda.net or Creation Club download
  flows ([LMC-NG-007](legacy-mod-compatibility.md#42-non-goals)). Each is a
  hosted service with an account; a user's existing downloads enter
  through local pins (DMH-PIN-001).
- **DMH-NG-009.** A peer-to-peer node, DHT participant or listening socket
  inside the engine binary.
- **DMH-NG-010.** Fallout 76 (§7.1).

---

## 5. Requirements by area

### 5.1 Addressing (`DMH-ADR`)

- **DMH-ADR-001.** A release manifest MUST list every artifact of the
  release with its package-relative path, its byte length and its digest.
  Today no manifest in the workspace carries a digest (§3.2 rows 1–2).
- **DMH-ADR-002.** The release address MUST be the digest of the release
  manifest's canonical encoding, and re-encoding a parsed release manifest
  MUST reproduce those bytes exactly. The encoding is owned by the Phase 0
  design.
- **DMH-ADR-003.** The release manifest MUST reference the package manifest
  as one of its artifacts by digest and MUST NOT restate it. The fetcher
  MUST reject a release whose `ModId` or version disagrees with its package
  manifest's (state **Mismatch**).
- **DMH-ADR-004.** A new version of a package MUST keep its `ModId` and
  MUST get a new release address
  ([ARC-001](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment)).
- **DMH-ADR-005.** Two releases that claim the same `ModId` and version
  with different release addresses MUST produce an identity-collision
  diagnostic naming both addresses and MUST NOT replace one another
  ([PKG-013](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements)).
- **DMH-ADR-006.** Sources MUST NOT contribute to the release address:
  adding or removing a mirror, CID or magnet for a release MUST leave its
  address unchanged.
- **DMH-ADR-007.** The fetcher MUST reject a release manifest with an
  absolute path, a `..` component, or two paths equal under case folding
  with `\` and `/` equivalent ([PKG-008](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements),
  [LMC-ID-009](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)).
- **DMH-ADR-008.** The digest of an executable component in a release
  manifest MUST be the same value the sandbox uses to declare it
  ([PKG-003](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements))
  and to invalidate its compilation cache
  ([SEC-016](../engine/sandboxed-linked-mods.md#102-requirements)); there is
  one component digest, not two.

### 5.2 Resolution and lock (`DMH-RES`)

- **DMH-RES-001.** Dependency closure MUST be computed with the
  `DependencyGraph` primitive in `crates/plugin/src/resolver.rs` that the
  record resolver and `ResolvedExtensionSet::resolve` already share. Today
  it is `pub(crate)` (§3.2 row 4); exposing it is part of the work, and a
  second graph implementation MUST NOT be written.
- **DMH-RES-002.** Every dependency edge in a release manifest MUST carry a
  `ModId` and a version range
  ([PKG-002](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements))
  and MAY carry a release-address pin that overrides the range. Today
  `PluginManifest` dependencies carry neither (§3.2 row 1);
  `ExtensionDependency` carries the range.
- **DMH-RES-003.** Resolution MUST be a pure function of the requested
  releases and the releases already known to the fetcher (the store plus
  feeds the user refreshed): the same inputs MUST produce a byte-identical
  release lock. Among known releases satisfying every range, the highest
  version MUST be selected.
- **DMH-RES-004.** A closure that would need two versions of one `ModId`
  MUST be a resolution error naming each requester and its range.
- **DMH-RES-005.** A dependency cycle MUST be reported with its path, as
  `dependency_order` already reports one
  (`cycles_missing_dependencies_and_bad_versions_fail_before_activation`,
  `crates/plugin/src/extension.rs`).
- **DMH-RES-006.** A required dependency with no known release MUST be
  reported with the chain of requesters; an optional one
  (`ExtensionDependency::optional`) MUST NOT fail resolution. What
  activation then does with the dependents is
  [LMC-ID-011](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)'s
  rule, not a second one.
- **DMH-RES-007.** Activation MUST read only the release lock and the
  store. The engine MUST NOT re-resolve, contact a feed or select a newer
  version at boot.
- **DMH-RES-008.** Resolution MUST decide which releases are present and
  MUST NOT decide record winners or plugin order: those stay profile order
  with masters first
  ([LMC-ID-003](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)).
- **DMH-RES-009.** The release lock MUST contribute to the profile
  fingerprint that saves store
  ([LMC-ID-007](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id),
  [LMC-SAV-001](legacy-mod-compatibility.md#510-saves-and-profile-changes-lmc-sav)),
  so a save names the exact releases it was made with.

### 5.3 Fetch and verification (`DMH-FET`)

- **DMH-FET-001.** The engine binary MUST NOT open a network connection to
  obtain, check or report on mod content. Today its only socket is the
  loopback debug server (§3.2 row 7); a dependency check in Phase 3 MUST
  keep any HTTP or peer-to-peer crate out of the engine binary's graph.
- **DMH-FET-002.** Every byte the fetcher stores MUST be verified against
  its digest before it becomes visible in the store; a mismatched blob
  MUST be discarded and its source named in the report.
- **DMH-FET-003.** Verification MUST NOT depend on the source. A release
  fetched from a local path, an HTTPS mirror, an IPFS trustless gateway or
  a BitTorrent swarm MUST pass or fail on the same digests. For a gateway
  the fetcher MUST also apply the gateway spec's own checks, under which
  "The Body hash MUST match the Multihash from the requested CID" and an
  incomplete CAR stream is a retrieval failure
  ([IPFS Trustless Gateway](https://specs.ipfs.tech/http-gateways/trustless-gateway/)).
- **DMH-FET-004.** The fetcher MUST stop reading an artifact once it
  exceeds the length its release manifest declares, and MUST cap the size
  of a release manifest it has not yet verified.
- **DMH-FET-005.** An interrupted transfer MUST resume without re-reading
  verified data, and partial data MUST live outside the store.
- **DMH-FET-006.** A profile whose lock is satisfied by the store MUST
  activate with networking unavailable to both the engine and the fetcher.
- **DMH-FET-007.** Every endpoint the fetcher contacts MUST come from the
  user's configuration or from a release manifest, feed or source the user
  named in that run. The shipped default configuration MUST list zero
  endpoints.
- **DMH-FET-008.** The fetcher MUST print, for each run, every endpoint it
  contacted, its §3.4 role and the input that named it. A request to an
  endpoint MUST NOT carry a user, machine or profile identifier, or the
  address of any release other than the one being fetched.
- **DMH-FET-009.** Sources MUST be tried in the user's configured order;
  a failing source MUST fall through to the next without discarding
  verified progress.
- **DMH-FET-010.** A BitTorrent source MUST contact only the trackers
  listed in the magnet or torrent the user named or in configuration, and
  only the DHT bootstrap nodes in the torrent's `nodes` key
  ([BEP 5](https://www.bittorrent.org/beps/bep_0005.html)) or in
  configuration. A magnet with neither and an empty configuration MUST
  fail with "no peer source" instead of contacting a default node.

### 5.4 Local store (`DMH-STO`)

- **DMH-STO-001.** The store MUST key blobs by digest, so an artifact
  shared by several releases is stored once.
- **DMH-STO-002.** A committed blob MUST be read-only and MUST be read by
  activation through its digest, so the bytes the profile pinned are the
  bytes that load
  ([ARC-006](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment)).
- **DMH-STO-003.** Blobs no release lock references MAY be removed, only
  by an explicit user command that lists what it removes.
- **DMH-STO-004.** A scrub command MUST re-hash every blob and report each
  one whose bytes no longer match its key.
- **DMH-STO-005.** The store MUST NOT write into a game installation's
  `Data` directory. Mounting stored content into a profile is the mod
  profile's job ([LMC-ID-012](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)).

### 5.5 Local pins (`DMH-PIN`)

- **DMH-PIN-001.** The fetcher MUST write a local pin for a mod archive or
  directory the user already has, with the digest of every file, a `ModId`
  derived as LMC-ID-012 derives a manifest-less root's identity (the
  directory name, case-folded), and state **Verified, unsigned**.
- **DMH-PIN-002.** Pinning the same bytes twice MUST produce the same
  release address.
- **DMH-PIN-003.** A release lock MAY mix local pins and fetched releases;
  activation MUST treat both identically.
- **DMH-PIN-004.** A local pin MUST NOT be served or published (§5.8).

### 5.6 Trust (`DMH-TRU`)

- **DMH-TRU-001.** A release manifest MAY carry signatures over its
  release address by publisher keys (format: decision Q4).
- **DMH-TRU-002.** Every release the fetcher reports MUST be in exactly
  one §3.3 trust state.
- **DMH-TRU-003.** The first key accepted for a `ModId` MUST be pinned in
  the user's configuration. A later release signed by another key MUST be
  reported **Key changed** and MUST NOT enter a lock without the user's
  confirmation, unless it carries a rotation statement signed by the
  pinned key.
- **DMH-TRU-004.** A signature MUST NOT grant a capability, raise a quota
  or skip a confirmation
  ([PKG-011](../engine/sandboxed-linked-mods.md#8-package-manifest-requirements),
  [SEC-013](../engine/sandboxed-linked-mods.md#102-requirements)).
- **DMH-TRU-005.** A grant to an executable package MUST be recorded
  against the release address it was given for. An update MUST show the
  capability diff against the locked release, and a capability the
  previous release did not request MUST stay denied until granted. Whether
  unchanged capabilities carry over is decision Q5.
- **DMH-TRU-006.** Fetched executable components MUST run under the same
  sandbox and limits as local ones
  ([SEC-001](../engine/sandboxed-linked-mods.md#102-requirements) to
  SEC-012); the distribution route MUST NOT create a second execution
  path.
- **DMH-TRU-007.** Fetched content files (plugins, NIFs, archives, scripts)
  MUST enter the engine through the same parsers as local ones, which
  already treat them as untrusted input
  ([sandbox §10.1](../engine/sandboxed-linked-mods.md#101-threat-model)).

### 5.7 Discovery (`DMH-DIS`)

- **DMH-DIS-001.** The fetcher MUST accept a release address, a release
  manifest file or a feed key as input, and MUST NOT search by name,
  keyword or category.
- **DMH-DIS-002.** A release address MUST print as one line with no
  whitespace, drawn from a case-insensitive multibase alphabet
  ([multiformats: multibase](https://github.com/multiformats/multibase)),
  so that it survives copy and paste through a web page or chat message.
- **DMH-DIS-003.** The fetcher MUST reject a feed record whose signature
  does not verify against the followed key, whose sequence number is lower
  than the last one accepted for that key, or whose validity has expired.
- **DMH-DIS-004.** Refreshing a feed MUST happen only on a user command
  (DMH-NG-004) and MUST NOT change a lock until the user accepts the lock
  diff (DMH-DX-002).
- **DMH-DIS-005.** The fetcher MAY import a third-party list of release
  addresses from a file or source the user names in that run; every entry
  is verified like any other and the list grants no trust state. An
  imported list MUST NOT be re-fetched without a user command, and MUST
  NOT be used to resolve a name, keyword or `ModId` to an address: the
  user selects entries by address, so a list is never a registry.

### 5.8 Publishing (`DMH-PUB`)

- **DMH-PUB-001.** Publishing the same package directory twice MUST
  produce the same release address. For editor output this composes with
  [WED-OUT-006](world-editor.md#56-output-plugin-wed-out)'s byte-identical
  saves.
- **DMH-PUB-002.** Publishing MUST refuse an artifact whose digest equals
  that of a file in the official content of any detected game installation
  (its official archives' entries and official loose files), and MUST name
  the matching official file.
- **DMH-PUB-003.** A release manifest MUST carry a redistribution
  declaration; a release without one MUST be treated as not
  redistributable, matching the Nexus rule that "If the author has not
  provided any permission information at all … you should assume that you
  must request permission from the author"
  ([Nexus Mods: File Submission Guidelines](https://help.nexusmods.com/article/28-file-submission-guidelines)).
- **DMH-PUB-004.** Publishing MUST write the release to the local store
  and print its address and the transport identifiers it can be served
  under (decision Q3). It MUST NOT upload anything.
- **DMH-PUB-005.** A Redux-native plugin written by the editor
  ([WED-OUT-001](world-editor.md#56-output-plugin-wed-out)) MUST be
  publishable without edits to its package manifest.

### 5.9 Legal (`DMH-LEG`)

- **DMH-LEG-001.** The fetcher MUST serve (seed, provide, mirror) a
  release only when its manifest is signed by a publisher key and its
  redistribution declaration permits third parties. Serving is decision
  Q7's opt-in.
- **DMH-LEG-002.** The engine and the fetcher MUST ship no release,
  address, address list, feed key or source of third-party content
  ([LMC-LEGAL-001](legacy-mod-compatibility.md#513-legal-lmc-legal)).
- **DMH-LEG-003.** No release manifest field, fetcher command or launcher
  screen MAY carry a price, payment or donation flow (DMH-NG-003).
- **DMH-LEG-004.** DMH-PUB-002 catches byte-identical game files only;
  content derived from game data (edited textures, re-exported meshes)
  cannot be detected and MUST be covered by the author's declaration. The
  fetcher MUST state that limit when publishing.

### 5.10 Reporting (`DMH-DX`)

- **DMH-DX-001.** An explain command MUST print, for each locked release,
  its address, `ModId`, version, trust state, the sources tried, and the
  requester chain that pulled it into the closure.
- **DMH-DX-002.** Changing a lock MUST print the lock diff first: releases
  added, removed and changed, with versions, trust-state changes and
  capability diffs.
- **DMH-DX-003.** When the launcher's mods screen exists
  ([Launcher P5](../engine/launcher.md#p5--mods)), it SHOULD show the same
  explain output and lock diff, invoking the fetcher rather than
  reimplementing it.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | First-party sharing uploads to a central platform: the FO4 Creation Kit builds the archives and sends "the package uploaded to Bethesda.net" ([FO4 CK: Sharing Mods](https://falloutck.uesp.net/wiki/Sharing_Mods)) | Releases addressed by hash; no platform (DMH-G-004) | Bethesda.net mod accounts for Redux content | No built-in way to find mods → discovery stays out of band, feeds for updates (DMH-DIS-001, decision Q8) |
| 2 | Mod sites host files; "Only the original author of a given file or asset can specify its distribution permissions" ([Nexus guidelines](https://help.nexusmods.com/article/28-file-submission-guidelines)) | Existing downloads enter as local pins; only author-declared releases are served (DMH-PIN-001, DMH-LEG-001) | Nothing: users keep downloading from mod sites | Most legacy mods will never be fetchable by address → local pins give them the same lock and reproduction guarantees (DMH-PIN-003) |
| 3 | Game mods "are for non-commercial use" and may be distributed only "to users who have purchased the Product" (Skyrim SE CK EULA, [UESP](https://en.uesp.net/wiki/Skyrim_Mod:EULA)); the Oblivion Construction Set and Skyrim LE Creation Kit terms say the same, "on a strictly non-commercial basis, to other authorized users who have purchased the Product" ([CS wiki: Eula](https://cs.uesp.net/wiki/CSwiki:Eula), [CK wiki: Eula](https://ck.uesp.net/wiki/Eula)) | No payment surface (DMH-LEG-003); serving is opt-in and author-declared (DMH-LEG-001) | — | A swarm cannot check that a peer owns the game → **Phase 0: verify** item 2; serving stays off by default (decision Q7) |
| 4 | A mod set is whatever files are on disk; nothing pins which bytes a profile used | Release lock pins every byte (DMH-RES-007) | Hand-kept "mod list" posts and manual re-download rituals | A lock pinned to a release nobody serves cannot be rebuilt elsewhere → the store keeps it locally (DMH-STO-003 never removes locked blobs) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Distribution moves bytes and never parses them, so no requirement here is
per game. The only per-game input is the official file set DMH-PUB-002
compares against, which comes from the installation profile (the
`game-detect` profiles registry the launcher and the engine both read,
[Launcher §2.6](../engine/launcher.md#26-profiles-file-agreement-5294)), and
the guard reads archives through the real readers, not a re-implementation
([Launcher §3.3](../engine/launcher.md#33-reuse-not-duplication)); a game
appears in the guard only through that data, never through a branch in
the fetcher. Each in-scope game is served in the wave the Legacy Mod
Compatibility PRD assigns it ([LMC §7.1](legacy-mod-compatibility.md#71-waves)),
which owns the wave table; a release is only useful once its content
activates.

| Game | In scope | Why |
|---|---|---|
| Oblivion | yes | Mods exist and LMC covers it; distribution never parses content |
| Fallout 3 | yes | As Oblivion |
| Fallout: New Vegas | yes | Engine reference title; first game A6 and A11 run on |
| Skyrim (LE / SE / AE) | yes | LE counted with SE, as in LMC §7.1 |
| Fallout 4 | yes | As Oblivion |
| Fallout 76 | no | Out of the mod promise ([LMC-NG-008](legacy-mod-compatibility.md#42-non-goals)) |
| Starfield | yes | As Oblivion |

### 7.2 Acceptance

No scenario checks in mod or game files. Fixture packages are generated
by the tests themselves (synthetic plugins, a minimal WebAssembly
component, random artifacts); game data is read from the user's
installation through the existing `BYROREDUX_FNV_DATA` style variables
and opt-in lanes. Every test name below is new unless marked existing.

| # | Scenario | Driver | Data | Passes when |
|---|---|---|---|---|
| A1 | Release manifest round trip | `cargo test` `release_manifest_reencodes_byte_identical` | generated | parse → encode reproduces the bytes and the address (DMH-ADR-002) |
| A2 | Sources do not move the address | `cargo test` `sources_do_not_change_release_address` | generated | same address with zero and three sources (DMH-ADR-006) |
| A3 | Tamper suite | `cargo test` `tampered_artifact_never_reaches_store` | generated; one flipped byte, one truncated, one oversized, one substituted per artifact | every case rejected, store unchanged, source named (DMH-FET-002, DMH-FET-004) |
| A4 | Closure determinism | `cargo test` `release_closure_is_deterministic` | generated diamond, chain, optional and cycle graphs | identical lock bytes across 100 runs with shuffled input order; cycle path matches the existing `cycles_missing_dependencies_and_bad_versions_fail_before_activation` shape (DMH-RES-003, DMH-RES-005) |
| A5 | One version per `ModId` | `cargo test` `two_versions_of_one_mod_in_closure_is_an_error` | generated | error names both requesters (DMH-RES-004) |
| A6 | Offline activation | new script `docs/smoke-tests/m72-offline-profile.sh` | FNV install; a generated content-only package | engine and `byro-dbg` run inside one `unshare -rn` network namespace with only loopback up; the engine reaches `--bench-hold` with the locked package active and `byro-dbg` attaches (DMH-FET-006) |
| A7 | Zero default endpoints | `cargo test` `default_fetcher_config_contacts_nothing` | none | request log empty for a fetch that the store satisfies, and for a fetch with no configured source (DMH-FET-007) |
| A8 | Source independence | `cargo test` `verification_is_source_independent` | generated; a loopback HTTP server in the test serving plain files and a CAR stream | same verdicts from path, HTTP and CAR for good and tampered bytes (DMH-FET-003) |
| A9 | Key change | `cargo test` `publisher_key_change_requires_confirmation` | generated keys | second key yields **Key changed**; signed rotation yields **Verified, trusted** (DMH-TRU-003) |
| A10 | Capability diff | `cargo test` `update_does_not_inherit_new_capabilities` | generated component with an added capability request | the new capability is denied after the update (DMH-TRU-005) |
| A11 | Game-data guard | opt-in `cargo test` `publish_refuses_official_game_bytes` | FNV install via `BYROREDUX_FNV_DATA`; one vanilla archive entry copied into a temp package | publish refused, official archive path named (DMH-PUB-002); repeated per game in Phase 4 |
| A12 | Reproducible publish | `cargo test` `publish_twice_same_address` | generated package; Phase 4 adds an editor output plugin | identical address (DMH-PUB-001, DMH-PUB-005) |
| A13 | Engine has no network client | `cargo test` `engine_dependency_graph_has_no_network_client` | `Cargo.lock` | no HTTP or peer-to-peer crate reachable from the `byroredux` package (DMH-FET-001) |
| A14 | No default peer source | `cargo test` `magnet_without_peer_source_contacts_nothing` | a generated magnet with no tracker and no nodes; empty configuration | fails with "no peer source", request log empty (DMH-FET-010) |

---

## 8. Metrics

The numbers live in ROADMAP. ROADMAP has no table for distribution yet;
Phase 1's exit adds a "Mod distribution" block to
[Project Stats](../../ROADMAP.md#project-stats), and every metric below is
published there.

| Metric | Definition | Granularity |
|---|---|---|
| Tamper catch rate | tamper cases rejected ÷ tamper cases injected (A3, A8) | per phase; must be 1 |
| Lock reproducibility | resolutions with byte-identical locks ÷ resolutions run (A4) | per phase |
| Offline activation | locked profiles that activate under A6 ÷ locked profiles tried | per game, per wave |
| Unrequested endpoints | endpoints contacted that did not come from configuration or a named input (A7) | per release of the fetcher; must be 0 |
| Guard coverage | official files refused ÷ official files injected (A11) | per game |
| Publish reproducibility | publishes with an unchanged address ÷ republishes of an unchanged package (A12) | per phase |

---

## 9. Delivery sequence

ROADMAP gates Tier 10 on "No active work until the engine ships something
playable" ([Tier 10](../../ROADMAP.md#tier-10--ecosystem-unlock-post-tier-4-horizon)).
Phase 0 is documentation and decisions and does not break that gate;
Phase 1 onwards waits for M46, which the Legacy Mod Compatibility PRD
delivers as its Phase 1
([LMC §9](legacy-mod-compatibility.md#9-delivery-sequence)), because a lock
belongs to a mod profile that does not exist yet.

**Phase 0 — Design and decisions (no engine code).** Write
`docs/engine/mod-distribution.md`: release manifest schema and canonical
encoding, store layout, feed record, fetcher command set, and how the
release manifest wraps each package-manifest kind. Resolve the §10
decisions. Close the **Phase 0: verify** items: (1) the redistribution and
commercial-use terms of the GECK (Fallout 3, New Vegas), Fallout 4
Creation Kit and Starfield Creation Kit EULAs, settled by reading each
EULA text as the Construction Set and Skyrim ones are read in §6 (the local
GECK and Fallout 4 CK wiki dumps carry no EULA page); (2) whether the Skyrim SE EULA's "only
permitted to distribute the Game Mods to users who have purchased the
Product" can be met by any peer-to-peer serving, settled by the owner's
legal reading of that clause; (3) the official definitions of the Nexus
per-file permission fields (upload, modification, conversion, asset use),
settled from the Nexus help centre's permissions article, since the file
guidelines quoted here do not define them; (4) that the chosen Ed25519
library's signatures verify with the reference `minisign` tool
([minisign](https://jedisct1.github.io/minisign/)), settled by a spike
outside the engine; (5) the maturity of BitTorrent v2 support in the Rust
ecosystem, settled by a survey of libraries against
[BEP 52](https://www.bittorrent.org/beps/bep_0052.html); (6) whether
IPNS feed resolution can be done through a user-configured gateway alone,
with no DHT participation, settled from the
[IPNS spec](https://specs.ipfs.tech/ipns/ipns-record/) and a gateway
test, which decides the §3.4 "IPNS resolution" row; (7) Q2's BLAKE3
gateway item, only if that alternative is weighed. Exit: the design
is checked in and reviewed, every §10 item carries **Decided**, and each
verify item is closed or filed as an issue.

**Phase 1 — Release manifest, store and local pins (offline).** Depends on
M46 / LMC Phase 1. DMH-ADR-001 to 008, DMH-STO-001 to 005, DMH-PIN-001 to
004, DMH-FET-002, DMH-FET-006, DMH-RES-009. No network code. Exit: A1, A2,
A3 (local-path source) and A6 pass on FNV; the ROADMAP Project Stats
block exists.

**Phase 2 — Closure and lock.** DMH-RES-001 to 008 over the shared
`DependencyGraph`, DMH-DX-001, DMH-DX-002. Exit: A4 and A5 pass; lock
reproducibility is published as 1.

**Phase 3 — Network fetch and trust.** The fetcher (decision Q6) with the
transports decision Q3 picks for the first wave; DMH-FET-001, 003, 004,
005, 007, 008, 009; DMH-TRU-001 to 007; DMH-DIS-001 to 005. Exit: A3, A7,
A8, A9, A10 and A13 pass; tamper catch rate 1 and unrequested endpoints 0
published.

**Phase 4 — Publishing and serving.** Depends on M50 for the editor
output path (DMH-PUB-005). DMH-PUB-001 to 005, DMH-LEG-001 to 004, the
remaining transport from decision Q3 with DMH-FET-010, serving per
decision Q7. Exit: A11 passes on every in-scope game installed on the test
machine, A12 passes for an editor output plugin, A14 passes, guard
coverage published per game.

**Phase 5 — Launcher integration.** DMH-DX-003 inside launcher P5, after
the load-order design that launcher §7 waits on. Exit: the launcher's mods
screen lists every locked release of a Wave 1 profile with its trust
state, and its lock diff matches the fetcher's output for the same change.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — ROADMAP wording for "mod-id = content hash".** Not a choice of
   identity model: ARC-001 and PKG-013 already keep `ModId` stable and
   distinct from content hashes (§3.1), and this PRD does not reopen them.
   Recommended: reword the M72 row to "release address = content hash;
   `ModId` stable", and the keep-list row to "Load-order-independent Form
   IDs (name- or manifest-derived `PluginId`)", matching §3.2 row 9 and
   [WED §3.4](world-editor.md#34-which-identity-the-output-plugin-follows).
   Alternative: keep the ROADMAP wording and treat it as a request to
   reopen ARC-001 in the sandbox design (not here); every update would
   then become a new mod, every `FormIdPair` its records carry would
   change, and an old save after an update would take LMC-SAV-002's
   removed-mod path for every updated mod.
2. **Q2 — Digest function and form.** Recommended: a SHA2-256 multihash of
   each artifact's whole bytes, independent of any transport's chunking.
   No transport carries that exact value (BitTorrent v2 roots a SHA2-256
   merkle tree over blocks, [BEP 52](https://www.bittorrent.org/beps/bep_0052.html);
   an IPFS file is a DAG of blocks), so the whole-file digest is checked
   after reassembly in addition to the transport's own checks; SHA2-256 is
   at least the function both transports use underneath. Alternative A:
   the IPFS UnixFS CID as identity; "Two identical files can produce
   different CIDs" because the CID depends on chunk size, DAG layout,
   codec, CID version and hash function
   ([IPFS: content addressing](https://docs.ipfs.tech/concepts/content-addressing/)),
   so identity would hinge on importer settings. Alternative B: a BLAKE3
   multihash; faster to compute, and BitTorrent v2 cannot share any work
   with it. **Phase 0: verify** whether the gateways a user would name
   serve BLAKE3 CIDs before this alternative is weighed.
3. **Q3 — Transports and their order.** Recommended: Phase 3 ships local
   path, HTTPS mirror and the IPFS trustless gateway, all plain HTTP
   clients with no daemon and no listening socket, each depending only on
   a server the user names (§3.4); Phase 4 adds BitTorrent v2 magnets,
   where "identical files always result in the same root hash"
   ([BEP 52](https://www.bittorrent.org/beps/bep_0052.html)), with
   metadata fetched from peers per
   [BEP 9](https://www.bittorrent.org/beps/bep_0009.html), and trackers
   and DHT bootstrap nodes only as DMH-FET-010 allows. Alternative A: an
   embedded IPFS/libp2p node; it joins a peer network through a bootstrap
   list that ships as "a default list of trusted peers"
   ([IPFS: bootstrap list](https://docs.ipfs.tech/how-to/modify-bootstrap-list/)),
   which breaks DMH-FET-007 unless the user supplies one, and needs a
   listening socket. Alternative B: BitTorrent first; strongest for large
   archives, but in a swarm "the downloaders upload to each other"
   ([BEP 3](https://www.bittorrent.org/beps/bep_0003.html)), so every
   fetcher becomes a server, which collides with decision Q7.
4. **Q4 — Signature scheme.** Recommended: offline Ed25519 publisher keys
   in a minisign-compatible format, which "uses the highly secure Ed25519
   public-key signature system" and carries a key ID and a trusted comment
   ([minisign](https://jedisct1.github.io/minisign/)), pinned per `ModId`
   on first use (DMH-TRU-003). Alternative A: Sigstore keyless signing;
   its certificates come from the Fulcio authority from an OpenID Connect
   identity and are logged in Rekor, run as a "public-good, non-profit
   service" ([Sigstore overview](https://docs.sigstore.dev/about/overview/)),
   which puts a hosted party in the trust path that the anti-scope
   excludes. Alternative B: no signatures, hash pins only; reproduction
   still holds, but an update can never be attributed to its publisher.
5. **Q5 — Grants across an executable update.** The distribution half of
   the sandbox design's open "Grant UX" question, which asks how
   "capability diffs, publisher trust, profile inheritance" are presented
   ([§20](../engine/sandboxed-linked-mods.md#20-open-design-questions),
   item 5); an answer here is folded back there. Recommended: a grant
   carries over only for a capability the locked release also requested,
   and only when the user accepts the lock diff that shows the changed
   component digest. Alternative A: any component digest change revokes
   every grant; safest, and every update re-asks for everything.
   Alternative B: grants keyed by `ModId` only; convenient, and a
   compromised update inherits all of its predecessor's authority.
6. **Q6 — Where the fetcher lives.** Recommended: a new library crate that
   holds every network dependency, with a thin CLI under `tools/` beside
   `byro-detect`; the launcher calls the library in P5 and the engine
   binary never depends on it (A13). Alternative A: inside the launcher;
   launcher §11 promises P1–P4 "make zero network requests", which then
   holds only by phase. Alternative B: inside the engine; contradicts
   DMH-FET-001 and puts a network stack beside the renderer.
7. **Q7 — Serving.** Recommended: off by default; an explicit per-release
   share command, limited to signed releases whose author declared
   third-party redistribution (DMH-LEG-001). Alternative: serve whatever
   was fetched, as the BitTorrent protocol's reciprocal uploading
   ([BEP 3](https://www.bittorrent.org/beps/bep_0003.html)) assumes;
   availability improves,
   and users then redistribute authors' work without the permission the
   Nexus guidelines say only the author can give.
8. **Q8 — How a user finds a mod with no index.** This is the core
   tension of a promise that rules out a central registry. Recommended:
   discovery stays out of band, where it is today: authors print release
   addresses and feed keys on mod pages, forums and chat; the fetcher
   resolves addresses and follows feeds and never searches; a community
   list of addresses can be imported once, as data, and never resolves a
   name (DMH-DIS-005), so it cannot become the registry the anti-scope
   rules out. The sandbox design asks the same question for WIT packages
   with a weaker bar, "without making one centralized registry mandatory"
   ([§20](../engine/sandboxed-linked-mods.md#20-open-design-questions),
   item 7); this PRD's bar is none, mandatory or default. Alternative A:
   keyword search over a distributed hash table; a global search space is
   an index nobody can moderate, so impersonation and spam have no remedy
   and takedowns have no address. Alternative B: ship a default community
   index URL or let imported lists resolve names; a central registry in
   all but name, against the anti-scope. Alternative C: addresses only,
   no feeds; simplest, and every update needs the user to find a new
   address by hand.
9. **Q9 — The M50 dependency.** Recommended: M50 gates only Phase 4's
   editor publishing (DMH-PUB-005), so Phases 1–3 need only M46; reword
   the ROADMAP row's "Depends on" accordingly. Alternative: keep the whole
   milestone behind M50, which delays offline locks and local pins, both
   useful for legacy mods that the editor never touches.

---

## 11. Document map

| Need | Go to |
|---|---|
| Mod package, `ModId`, package manifest rules, signatures, sandbox security | [sandboxed-linked-mods.md](../engine/sandboxed-linked-mods.md) |
| Mod profile, plugin identity, order, ownership, fingerprint, missing masters, legal | [legacy-mod-compatibility.md](legacy-mod-compatibility.md) |
| `PluginManifest`, `DependencyResolver`, Form ID layers | [plugin-loading.md](../engine/plugin-loading.md) |
| Editor output plugin that Phase 4 publishes | [world-editor.md](world-editor.md) |
| Launcher, boot request, the deferred mods screen, zero-network promise | [launcher.md](../engine/launcher.md) |
| Smoke-test index the new A6 script joins | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Live milestone state, anti-scope, keep list, published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: §3.4 third-party server dependence per option (anti-scope "Hosted online services"), DMH-FET-010 and A14 for tracker / DHT bootstrap endpoints, DMH-DIS-005 lists never resolve names, `PluginId` stated precisely against the keep list (§3.1, §3.2 row 9), Q1 cites ARC-001 / PKG-013 instead of re-deciding, Q5 and Q8 cite sandbox §20, CS and Skyrim LE CK EULAs cited, BEP 3 / BEP 5 / IPFS bootstrap citations, Q2 BLAKE3 claim corrected, per-game table links LMC waves instead of copying them, DMH-DIS-002 and DMH-FET-008 made observable | claude: revise |
