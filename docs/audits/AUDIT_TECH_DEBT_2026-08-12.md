# ByroRedux Tech-Debt Audit — 2026-08-12

Leg of the `ui-deep` audit suite. Depth: deep (per-instance triage).
Prior report: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`.

## Scope

Standard 9-dimension `/audit-tech-debt` sweep, **weighted toward
`crates/ui/`**. `crates/ui/` (Scaleform/SWF, R4 + M48 host layer) is one of
the six **un-owned subsystems** listed in `.claude/commands/_audit-common.md` —
there is no `/audit-ui` skill, so this preset is its only coverage. Prior
tech-debt reports touched it exactly twice, both incidentally
(`AUDIT_TECH-DEBT_2026-08-07.md` at `crates/ui/Cargo.toml` for a dependency
check, `AUDIT_TECH_DEBT_2026-08-02.md` at `crates/ui/src/host/tests.rs` for an
`#[ignore]` delta count). **Nothing has ever examined `crates/ui/src/`'s
contents.** This report is its first read.

Findings are tagged **[UI]** or **[GENERAL]** throughout.

Deduplicated against `/tmp/audit/issues.json` (400 issues, all states) and
every prior `docs/audits/AUDIT_TECH*DEBT_*.md`. Zero existing issues in the
baseline touch `crates/ui/src/`. The 21 issues filed today from the
texture-roles audit (#2693-#2713) were checked; two findings below are called
out as *rhyming with* #2712 and #2696/#2703 respectively, but neither
re-reports them.

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 8 |
| **Total** | **10** |

Seven findings are UI-scoped, three are general. Dimensions 4 (audit-finding
rot), 5 (stale markers) and 6 (stub implementations) returned **zero**
findings.

### Two brief premises did not survive verification

The suite brief framed two `crates/ui/` artifacts as drift-prone. Both
premises are **wrong as stated**, and saying so is more useful than filing
against them:

1. **`crates/ui/src/avm2_host.rs` is not generated code.** The brief describes
   it as "the *generated* AVM2 forwarding adapter" and asks whether "the
   generator, its input, and the checked-in output are still in sync." There is
   no checked-in output. `avm2_host.rs` is hand-written Rust that **emits ABC
   bytecode at runtime**: `build_adapter_abc()` constructs a `swf::avm2::types::AbcFile`
   in memory and `inject_host_object_adapter()` splices it into the SWF *after*
   `decompress_swf`/`parse_swf` and before Ruffle sees the movie. Nothing is
   serialized to disk, so there is no generator/input/output triple that can
   desynchronize, and no "how to regenerate" doc is needed or missing. The real
   drift surface in that file is different and is filed below as TD7-2026-08-12-01
   (hand-numbered constant-pool indices) and TD8-2026-08-12-01 (dead pool entries
   those indices make un-deletable).

2. **The 74- and 138-method catalogs *are* count-pinned by a test.**
   `crates/ui/src/host/tests.rs:246` (`skyrim_catalog_is_pinned_sorted_and_profile_specific`)
   asserts `catalog.len() == 74`, `fallout.len() == 138`, the Skyrim request
   count `== 12`, and — importantly — strict sortedness of **both** lists via
   `windows(2).all(|m| m[0].name < m[1].name)`. That sortedness assertion is
   load-bearing, not cosmetic: `ScaleformHostCatalog::find` uses
   `binary_search_by`, so an out-of-order insert would silently return `None`
   for real methods. Verified by running the suite (16 passed, 3 ignored). Both
   counts also match `ROADMAP.md:627-628`, `docs/feature-matrix.md:166-168`,
   and `docs/engine/ui.md` — no count drift anywhere.

What the catalogs *do* have is a **content** defect the count pin cannot see
(TD8-2026-08-12-02) and a **one-directional, `#[ignore]`d** drift guard that is
structurally incapable of finding it (TD9-2026-08-12-01).

## Baseline Snapshot (for the next audit's diff)

```
TODO/FIXME/HACK/XXX:    19   (Dim 5: 0 actionable — unchanged population vs 08-07)
allow(dead_code):       52   (was 48; all 4 new are byroredux/src/interaction.rs — see note)
unimplemented!/todo!(): 0    (Dim 6: still 0, incl. first-ever sweep of crates/ui/)
#[ignore] (.rs attrs):  138  (counted with --include='*.rs' over crates/ byroredux/ tools/;
                              NOT comparable to 08-07's 345, which counted textual hits
                              repo-wide including prose — see open #2262/TD4-002)
files >2000 LOC:        10   (was 9; NEW: byroredux/src/main.rs, 1958 -> 2054)
crates/ui/ tests:       19   (16 default + 3 ignored installed-corpus; all pass)
```

Window audited: 86 commits / 427 files / +35,021 -4,086 since
`AUDIT_TECH-DEBT_2026-08-07.md`.

## Top Quick Wins

Trivial/small effort, immediate payoff.

1. **TD3-2026-08-12-01** — three live "100-byte `Vertex`" doc sites; the value
   has been 104 and test-pinned since `cd2b5fe4`. One is inside the renderer
   itself. Highest-value trivial fix in this sweep.
2. **TD8-2026-08-12-02** — delete `functiononGPSModeButtonClicked` from the FO4
   catalog and add the real `onGPSModeButtonClicked` it was mangled from.
3. **TD3-2026-08-12-02** — `ROADMAP.md`'s M48 row still lists input routing as
   remaining work 16 days after it shipped.
4. **TD2-2026-08-12-01** — extract one `abc_payload(&Tag) -> Option<&[u8]>`
   helper; removes 4 copies of the same match plus 2 `unreachable!()` arms.
5. **TD3-2026-08-12-03** — `docs/engine/ui.md` says the executable adds "three"
   winit-translation tests; `byroredux/src/ui_input.rs` has four.

## Top Medium Investments

1. **TD8-2026-08-12-03** — decide the fate of the un-consumed R4/M48 host
   bridge. Either wire `drain_calls()` into the frame loop or bound the queue;
   today it grows without limit behind a shipped CLI flag.
2. **TD9-2026-08-12-01** — make the catalog drift guard bidirectional and give
   Skyrim an equivalent. This is the structural fix that would have caught
   TD8-2026-08-12-02 at authoring time.
3. **TD7-2026-08-12-01 + TD8-2026-08-12-01** (do together) — replace
   `build_adapter_abc`'s hand-numbered constant-pool indices with a builder, so
   the dead pool entries become deletable.
4. **TD1-2026-08-12-01** — `byroredux/src/main.rs` split along the axis the
   skill already names (event loop / system registration / boot+config).

---

# Findings

## MEDIUM

### TD3-2026-08-12-01: Three live "100-byte `Vertex`" doc sites; the pinned value is 104 [GENERAL + UI]

- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/engine/ui.md:271`, `docs/engine/testing.md:88`, `crates/renderer/src/vulkan/pipeline.rs:806`
- **Status**: NEW
- **Description**: All three describe the scene `Vertex` as 100 bytes. The
  live value is **104**, pinned by `crates/renderer/src/vertex.rs:331`
  (`assert_eq!(size_of::<Vertex>(), 104)`), and has been since the RGBA-color
  widening in `cd2b5fe4`. The sibling doc comment at `vertex.rs:278` correctly
  says 104, so the *same crate* now disagrees with itself.
- **Evidence**:
  - `docs/engine/ui.md:271` — "rather than the full 100-byte scene `Vertex`"
  - `docs/engine/testing.md:88` — "`vertex.rs` pins the 100-byte stride / 9 attribute descriptions"
  - `crates/renderer/src/vulkan/pipeline.rs:806` — "instead of the full 100-byte Vertex (post-M-NORMALS, #783)"
  - `crates/renderer/src/vertex.rs:278` — "Using this instead of the full 104-byte `Vertex` (post-" ✅
- **Impact**: A wrong number in a vertex-input layout contract. The
  `UiVertex`-vs-`Vertex` split these three sites explain exists *because* of
  the size delta, so the stated rationale is quantitatively wrong. Anyone
  sizing a staging buffer or reasoning about skinned-vertex stride from the
  docs gets a 4-byte-per-vertex error.
- **Related**: Promoted per the severity table's "stale GPU-struct size in a
  doc comment (lockstep-drift bait)" trigger — `Vertex` is not literally in
  that row's `GpuCamera`/`GpuInstance`/`GpuMaterial` list, but it is the same
  class of test-pinned `#[repr(C)]` layout contract. The **CLAUDE.md** instance
  of this same stale number was found by `AUDIT_PERFORMANCE_2026-07-25` (D6-02)
  and fixed; that pass did not sweep for siblings, and these three survived.
  Same doc-rot class as today's #2696 / #2703, different subsystem.
- **Suggested Fix**: `100` → `104` at all three sites. Then grep
  `100-byte\|100 byte` once more — the remaining hits
  (`crates/plugin/src/esm/records/items.rs:205`, `crates/nif/src/header.rs:575`,
  `crates/nif/src/blocks/dispatch_tests/havok.rs:33`) are unrelated record/block
  sizes and are correct.
- **Effort**: trivial

### TD8-2026-08-12-03: The entire R4/M48 Scaleform host bridge has no engine consumer, and its call queue is unbounded [UI]

- **Severity**: MEDIUM
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/ui/src/host.rs:131` + `:221` + `:313`, `crates/ui/src/lib.rs:63-150`, `crates/ui/src/navigator.rs`
- **Status**: NEW
- **Description**: `byroredux/` never calls `UiManager::host_bridge()`,
  `ScaleformHostBridge::drain_calls()`, `UiManager::invoke_callback()`,
  `UiManager::load_swf_with_profile()`, or
  `UiManager::load_swf_from_resource_provider()`. The binary's entire use of
  `crates/ui` is `UiManager::new` + `load_swf` + `tick`/`render` +
  `handle_input`/`set_mouse_in_stage`/`has_input_focus`. Every ActionScript →
  engine call the M48 work exists to deliver is recorded and then never read.
- **Evidence**:
  - `grep -rn "host_bridge\|drain_calls\|invoke_callback" byroredux/src` → the
    only hits are `ui_manager.handle_input` / `set_mouse_in_stage` /
    `has_input_focus`; zero bridge hits.
  - `crates/ui/src/host.rs:313` — `state.calls.push_back(ScaleformHostCall { … })`
    on every `ExternalInterfaceProvider::call_method`, into a
    `VecDeque<ScaleformHostCall>` (`:131`) whose only drain is the
    never-called `drain_calls()` (`:221`).
  - `byroredux/src/scene.rs:1135` — `--swf <path>` is a real, documented flag
    (`docs/engine/game-loop.md:39`, `README`-level usage in `docs/engine/ui.md:321`).
  - `crates/ui/src/navigator.rs` (564 LOC) is reachable only through
    `SwfPlayer::from_resource_provider`, whose only non-test caller is
    `UiManager::load_swf_from_resource_provider` — itself uncalled. So the
    archive-backed navigator is, in the shipped binary, unreachable code.
- **Impact**: Two distinct costs. (a) **Unbounded growth**: a menu loaded via
  `--swf` that calls `GameDelegate.call` / `BGSCodeObj.*` per frame accumulates
  one heap-allocated `ScaleformHostCall` (several `String`s + a
  `Vec<ScaleformValue>`) per call for the process lifetime. Bounded in practice
  only by how long a dev leaves the flag on. (b) **Un-exercised surface**: the
  response/handler API (`set_response`, `set_response_values`,
  `set_response_handler`, `register_method`) and the whole navigator have no
  production caller, so nothing but the crate's own tests would notice them
  regressing.
- **Related**: Same class as today's **#2712** (uploaded-but-never-sampled
  data) — data produced through a full pipeline that no consumer reads —
  reached here from the opposite end (the *engine* never reads what the *UI*
  produces, rather than the shader never sampling what the CPU uploads).
  Distinct code, distinct subsystem: this is a new finding, not a re-file.
- **Suggested Fix**: Short term, cap `BridgeState::calls` (drop-oldest with a
  warn counter) so the flag is safe to leave on; the queue is explicitly
  documented as drain-based, so a bound is a behavior-preserving guard. Medium
  term, drain it in the same main-loop block that already calls
  `ui.tick(dt)` / `ui.render()` and log unhandled methods — that also turns
  `unknown_methods()` into a live diagnostic instead of a test-only one.
  Do **not** delete the navigator or response API: they are the substrate the
  remaining M48 slices are specified against (`ROADMAP.md:628`).
- **Effort**: small (bound) / medium (wire the drain)

## LOW

### TD8-2026-08-12-02: FO4 catalog carries `functiononGPSModeButtonClicked`, a whitespace-collapse extraction artifact — and is therefore missing the real method [UI]

- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/ui/src/catalog.rs:280`
- **Status**: NEW
- **Description**: `FALLOUT4_BGS_CODE_OBJECT_METHODS` contains
  `ScaleformHostMethod::command("functiononGPSModeButtonClicked")`. That is
  `function onGPSModeButtonClicked` with the space removed — an artifact of
  scraping the F4CF/Interface ActionScript sources. It is the only entry in
  either catalog that is not a plain Camel/camelCase identifier; all 137 other
  FO4 entries and all 74 Skyrim entries are well-formed. The genuine method
  `onGPSModeButtonClicked` is **absent** — sorted order would place it between
  `onFadeDone` (`:294`) and `onGridAddedToStage` (`:295`), and it is not there.
- **Evidence**: `crates/ui/src/catalog.rs:280` verbatim; the sorted-window
  assertion at `host/tests.rs:271-274` passes because the mangled name still
  sorts correctly, so sortedness cannot detect it, and `len() == 138` at `:268`
  counts it as a valid entry.
- **Impact**: Two-sided, both small. (1) `build_adapter_abc` emits one
  forwarding method + one method body + one trait + two constant-pool strings
  per catalog entry, so every FO4 SWF the engine patches carries a dead helper
  and a dead `BGSCodeObj.functiononGPSModeButtonClicked` property. (2) The
  Pip-Boy map's real GPS-mode button, when it fires, normalizes to
  `onGPSModeButtonClicked`, misses the catalog, and is classified
  `ScaleformHostDispatch::Unknown` — logged as an unknown method rather than
  queued. Neither breaks anything today (nothing drains the queue — see
  TD8-2026-08-12-03), but the catalog is documented as the recognition surface
  future work is specified against.
- **Related**: Only detectable by TD9-2026-08-12-01's guard if that guard ran
  and were bidirectional; it is neither.
- **Suggested Fix**: Replace the entry with `onGPSModeButtonClicked` (same
  sort position, so `len()` stays 138 and no test needs touching). While there,
  re-run the extraction with a whitespace-tolerant pattern to confirm this was
  the only collapse artifact — the four intentional case-pairs
  (`CloseMenu`/`closeMenu`, `GetButtonFromUserEvent`/`getButtonFromUserEvent`,
  `OnAcceptPress`/`onAcceptPress`, `PlaySound`/`playSound`) are documented as
  deliberate at `docs/engine/ui.md` and must be preserved.
- **Effort**: trivial

### TD7-2026-08-12-01: `build_adapter_abc` hand-numbers ~38 constant-pool indices against two Vec literals declared 40-90 lines earlier [UI]

- **Severity**: LOW
- **Dimension**: 7 (Magic Numbers & Hardcoded Constants)
- **Location**: `crates/ui/src/avm2_host.rs:516-580` (declarations) and `:582-880` (uses)
- **Status**: NEW
- **Description**: The generated ABC's constant pool is built as two literal
  `Vec`s — `strings` (27 entries, `:516`) and `multinames` (17 entries, `:545`)
  — and then referenced by **1-based positional literals**: 21 distinct
  `Index::new(N)` values plus 17 `qname(namespace, name)` literal pairs, plus
  four `Namespace::Package(Index::new(N))` entries at `:857-862`. Correctness
  depends entirely on nobody inserting into the middle of either `Vec`. The
  only defence is a trailing-comment column (`qname(1, 14),  // BGSCodeObj`).
- **Evidence**: Literal `Index::new` values inside `build_adapter_abc`:
  `{0,1,2,3,4,5,10,11,12,13,14,16,17,18,19,20,22,24,25,26,27}`. Cross-checked
  every one against the two `Vec` literals during this audit: **all currently
  correct**, and every trailing comment matches. The file already provides
  `add_string` / `add_multiname` (`:889`, `:894`) which return the correct
  `Index` — but they are used only for the per-catalog-method entries appended
  in the loop, never for the fixed prefix.
- **Impact**: No live defect. The failure mode is silent and delayed: inserting
  one string in the middle shifts every subsequent literal by one, producing an
  ABC that still parses (so `generated_adapter_is_valid_abc_with_one_helper_per_method`
  at `:934` still passes — it asserts *counts*, not *identities*) but forwards
  calls under wrong names. Only the `#[ignore]`d installed-corpus tests would
  catch it. This is also the mechanical reason TD8-2026-08-12-01's dead entries
  cannot simply be deleted.
- **Related**: TD8-2026-08-12-01 (same site, same root cause). One test does
  pin a raw index — `assert_eq!(callback_names, [22])` at `:984` — which is a
  guard, but one whose failure message names an integer rather than a symbol.
- **Suggested Fix**: Route the fixed prefix through the existing
  `add_string`/`add_multiname` helpers into named `let` bindings, exactly as the
  per-method loop already does, and delete every literal `Index::new` except
  `Index::new(0)` (the ABC "any" sentinel). This makes insertion order-independent
  and lets `:984` assert against `loaded_callback_string` instead of `22`.
- **Effort**: small

### TD8-2026-08-12-01: Six multinames, nine strings and two namespaces in the generated ABC are dead — leftovers from the abandoned `LoaderInfo` approach [UI]

- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/ui/src/avm2_host.rs:516-563`, `:857-862`
- **Status**: NEW
- **Description**: The constant pool declares entries no emitted opcode ever
  references. Multiname slots **2** (`flash.display::LoaderInfo`), **6**
  (`getLoaderInfoByDefinition`), **7** (`addEventListener`), **8** (`target`),
  **9** (`content`) and **15** (`flash.utils::setTimeout`) are never bound to a
  local and never appear in any `Op`. The strings backing them — plus
  `"complete"` (1-based 13), which no `qname` even references — and the
  `flash.display` / `flash.utils` namespaces are dead with them.
- **Evidence**: The `let` bindings at `:564-580` cover multiname positions
  1, 3, 4, 5, 10, 11, 12, 13, 14, 16, 17 (plus `root_slot`, appended
  dynamically). Positions 2, 6, 7, 8, 9, 15 have no binding and no literal use
  anywhere in the function. The module's own history explains why: the doc at
  `docs/engine/ui.md` states the adapter patches the lifecycle constructor
  specifically to **avoid** "Ruffle's intentionally stubbed
  `LoaderInfo.getLoaderInfoByDefinition` root lookup" — i.e. the
  `LoaderInfo`/`addEventListener`/`target`/`content`/`complete` chain is the
  *superseded* strategy's vocabulary, and the `setTimeout` chain is a deferral
  mechanism that also went unused.
- **Impact**: Small and bounded — ~150 bytes of dead constant pool written into
  every FO4 SWF the engine patches, and ~15 lines of misleading declaration
  suggesting a load-event path the adapter does not take. Zero runtime cost
  beyond parse.
- **Related**: TD7-2026-08-12-01 is why this has not already been cleaned:
  deleting any of these entries renumbers everything after it, so the cleanup
  is gated on the index refactor. Do them in one commit.
- **Suggested Fix**: Delete the six multinames, the nine strings and the two
  namespaces **after** TD7-2026-08-12-01 lands. Verify with
  `generated_adapter_is_valid_abc_with_one_helper_per_method` plus one
  `--ignored` run of `installed_fallout4_host_calls_are_cataloged`.
- **Effort**: trivial once TD7-2026-08-12-01 is done; do not attempt before.

### TD9-2026-08-12-01: The only catalog-drift guard is `#[ignore]`d *and* one-directional, so dead catalog entries are structurally undetectable [UI]

- **Severity**: LOW
- **Dimension**: 9 (Test Hygiene)
- **Location**: `crates/ui/src/avm2_host.rs:988-1017`, `crates/ui/src/host/tests.rs:246-283`
- **Status**: NEW
- **Description**: `installed_fallout4_host_calls_are_cataloged` extracts the
  BGSCodeObj method names actually referenced by three installed FO4 movies and
  asserts `methods ⊆ catalog`. It never asserts anything about catalog entries
  the corpus does *not* reference, so a bogus entry passes forever. It is also
  `#[ignore = "requires an installed Fallout 4 corpus"]`, so it runs only on an
  explicit `--ignored` invocation on a machine with FO4 installed. Skyrim has
  **no** equivalent at all — `SKYRIM_SKYUI_METHODS` is pinned to a SkyUI git
  tree (`835428728e…`) by comment only, with nothing checking the tree still
  says that.
- **Evidence**: `catalog.contains(method)` filter at `avm2_host.rs:1006` — the
  assertion is `unknown.is_empty()`, one direction only. The default-suite
  catalog test (`host/tests.rs:246`) asserts length, sortedness, one membership
  and one kind — all of which TD8-2026-08-12-02's mangled entry satisfies.
  Verified empirically: the full suite passes (16 passed, 3 ignored) *with* the
  bad entry present.
- **Impact**: The gate that exists reads as coverage but cannot fail on the one
  defect class it looks like it addresses. This is how a malformed entry
  survived into a checked-in, doc-referenced, ROADMAP-cited 138-method catalog.
- **Related**: Rhymes with today's **#2702** (tests that re-implement production
  logic and therefore cannot fail) — same outcome (a test that cannot detect
  its nominal target), different mechanism (asymmetric assertion + opt-in
  gating rather than logic duplication). Not a re-file.
- **Suggested Fix**: Add the reverse assertion behind the same `#[ignore]` —
  report catalog entries no representative movie references, as a *warning
  list* rather than a hard failure (legitimate entries exist for menus outside
  the three-movie sample). Separately, add a default-suite well-formedness
  assertion that every catalog name matches `^[A-Za-z][A-Za-z0-9]*$` **and**
  contains no embedded ActionScript keyword (`function`, `var`, `return`) —
  that single cheap check catches TD8-2026-08-12-02's whole artifact class
  without needing a game install.
- **Effort**: small

### TD2-2026-08-12-01: The `Tag::DoAbc`/`DoAbc2` payload match is written out four times, plus two `unreachable!()` arms for the same discriminant [UI]

- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `crates/ui/src/avm2_host.rs:58-61`, `:77-81`, `:108-112`, `:209-213`
- **Status**: NEW
- **Description**: Four copies of

  ```rust
  let data = match tag {
      Tag::DoAbc(data) => Some(*data),
      Tag::DoAbc2(do_abc) => Some(do_abc.data),
      _ => None,
  };
  ```

  The `:108` copy is the non-`Option` variant and carries
  `_ => unreachable!("root ABC index must reference an ABC tag")`; `:124` has a
  second `unreachable!()` for the same discriminant in the replacement-tag
  match. CLAUDE.md's global rule is explicit that logic is improved in place,
  not duplicated.
- **Impact**: Low but real: any future SWF tag that can carry ABC (or a change
  in the pinned `swf` crate's tag enum) must be added in four places, and two
  of them panic rather than degrade if missed.
- **Suggested Fix**: One private `fn abc_payload<'a>(tag: &Tag<'a>) -> Option<&'a [u8]>`;
  the three `Option` sites become `abc_payload(tag)`, and the `:108` site becomes
  `abc_payload(&movie.tags[root_abc_index]).ok_or_else(…)?`, converting one of
  the two `unreachable!()` panics into an ordinary `Result` — consistent with
  the rest of the module, which is `Result`-based throughout.
- **Effort**: trivial

### TD3-2026-08-12-02: `ROADMAP.md`'s M48 row still lists input routing as remaining work 16 days after it shipped; `feature-matrix.md` has no input row at all [UI]

- **Severity**: LOW
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `ROADMAP.md:628`, `docs/feature-matrix.md:160-170`
- **Status**: NEW
- **Description**: `ROADMAP.md:628` ends "Remaining work: method behavior and
  `_global.gfx` stubs, font fidelity/**input**/menu lifecycle, and Papyrus/ECS ↔
  UI callbacks." Input routing landed in `3ea5e275` (2026-07-27, *feat(ui):
  implement input routing for Scaleform menus with winit integration*), shipping
  `crates/ui/src/input.rs`, `byroredux/src/ui_input.rs`, focus transfer, modal
  capture ahead of world controls, and window→movie coordinate scaling.
  `docs/engine/ui.md` documents all of it as shipped. `ROADMAP.md` was edited as
  recently as 2026-08-11 without the row being reconciled. Separately,
  `docs/feature-matrix.md`'s UI table has six rows and none of them mentions
  input/focus routing, so the matrix under-reports the subsystem.
- **Impact**: Exactly the failure the skill's Dim 3 recipe targets — "flag any
  row whose status contradicts the crate that implements it." A reader planning
  M48 work would re-scope a slice that is already done.
- **Related**: Verified the *counts* in the same rows are fine — `ROADMAP.md:627-628`
  and `feature-matrix.md:166-168` both say 74/138, matching `catalog.rs`. Only
  the remaining-work list is stale. Same class as #2416 (`feature-matrix.md`
  stale `hkx` rows, still OPEN).
- **Suggested Fix**: Drop "input" from the `ROADMAP.md:628` remaining-work list
  (keep font fidelity, menu lifecycle, `_global.gfx`, Papyrus↔UI — those are
  genuinely open) and add a `Scaleform menu input routing + modal focus | ✓ M48`
  row to `docs/feature-matrix.md`'s UI table.
- **Effort**: trivial

### TD3-2026-08-12-03: `docs/engine/ui.md` undercounts the executable's winit-translation tests [UI]

- **Severity**: LOW
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/engine/ui.md` ("Tests" section)
- **Status**: NEW
- **Description**: "The UI crate has 16 default tests plus three ignored
  installed-corpus smokes; the executable adds **three** winit-translation
  tests." The crate half is exactly right (verified: 16 passed, 3 ignored). The
  executable half is not — `byroredux/src/ui_input.rs` has **four** `#[test]`s.
- **Impact**: Trivial in isolation. Filed because this doc's test-count
  paragraph is otherwise precise enough to be used as a baseline, and a
  known-wrong number in it devalues the rest.
- **Suggested Fix**: "three" → "four".
- **Effort**: trivial

### TD1-2026-08-12-01: `byroredux/src/main.rs` crossed 2000 LOC, taking the oversized set from 9 to 10 [GENERAL]

- **Severity**: LOW
- **Dimension**: 1 (File / Function / Module Complexity)
- **Location**: `byroredux/src/main.rs` (2054 LOC)
- **Status**: NEW
- **Description**: 1958 → 2054 LOC across the 86-commit window since the
  08-07 report, crossing the 2000-LOC Session-34 split threshold. The other
  nine members of the oversized set are unchanged in membership. Roughly 60 of
  those lines are `#[cfg(test)]` (`:1991`, `:2034`), so this is genuine
  production growth, unlike the test-bulk crossings tracked as TD1-004
  (`save_io.rs`) and TD1-009 (`vulkan/material.rs`).
- **Impact**: Standard oversized-file tax. `main.rs` is the file every new
  system-registration or event-loop change touches, so it is a merge-conflict
  hotspot as well as a review-cost one.
- **Related**: TD1-001..012 in `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`
  (the nine-file set); this is the tenth. Open siblings #2410 (TD1-007,
  `cell_loader/spawn.rs`).
- **Suggested Fix**: Split along the axis the skill already prescribes for this
  file — `App`/`ApplicationHandler` winit event loop vs. system registration vs.
  boot/config. The repo already has `byroredux/src/boot.rs`, so the third of
  those has a landing site.
- **Effort**: medium

### TD8-2026-08-12-04: Four new `#[allow(dead_code)]` in `byroredux/src/interaction.rs` — justified, but the enum-level one is broader than needed [GENERAL]

- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `byroredux/src/interaction.rs:34`, `:85`, `:139`, `:148`
- **Status**: NEW (tracking)
- **Description**: The repo-wide `allow(dead_code)` count moved 48 → 52 since
  08-07; all four additions are here. Three are the **correct narrow form**
  (`#[cfg_attr(not(test), allow(dead_code))]` on `bind_key`, `is_held`,
  `was_released` — used by tests, not yet by production). The fourth is a
  blanket `#[allow(dead_code)]` on the whole `InputAction` enum, justified
  inline as "Mouse/gamepad sources for these declared actions land next"; four
  of its eleven variants (`Attack`, `Block`, `Inventory`, `Pause`) have no
  producer yet.
- **Impact**: None today. Flagged so the next sweep can tell whether the
  enum-level allow outlived its stated driver: once mouse/gamepad sources land,
  it should be deleted rather than inherited.
- **Suggested Fix**: No action now. On the next audit, check whether the
  gamepad/mouse source work has landed; if it has and the attribute survives,
  it becomes a real finding.
- **Effort**: n/a (tracking only)

---

## Verified Clean

- **Dimension 4 (Audit-Finding Rot)** — `.claude/commands/_audit-validate.sh`:
  1193 refs across 26 skill files, **all valid**, zero STALE, zero symbol
  advisories. No action.
- **Dimension 5 (Stale Markers)** — 19 raw hits, **0 actionable**, and the
  population is unchanged from 08-07: 13 are the ESM `XXXX` extended-size
  protocol tag (`crates/plugin/src/esm/reader.rs`, `records/misc/magic.rs`,
  `esm/cell/mod.rs`, `esm/cell/wrld.rs`), 2 document a *reference
  implementation's* FIXME (`crates/bgsm/src/bgem.rs:137`,
  `crates/nif/src/blocks/bs_geometry.rs:596`), and 1 records a TODO that was
  *closed* (`byroredux/src/scene.rs:1091`). **`crates/ui/` contains zero
  markers** — first-ever check of that crate.
- **Dimension 6 (Stub Implementations)** — `unimplemented!()` / `todo!()` /
  `panic!("not ` still **0** repo-wide, now including the first sweep of
  `crates/ui/`. The two `unreachable!()` in `avm2_host.rs` are same-discriminant
  match exhaustiveness, folded into TD2-2026-08-12-01 rather than reported as
  stubs. No console command in `byroredux/src/commands/` no-ops or prints TODO.
- **`crates/ui/` internal hygiene** — zero `#[allow(dead_code)]`, zero
  `#[deprecated]`, zero `// removed:` breadcrumbs, zero `_`-prefixed
  refactor-leftover params, and one legitimately named tunable
  (`MAX_ARCHIVE_PRELOAD_PASSES`, `player.rs:29`). The remaining consts are
  namespaced adapter symbols (`__byro_fallout4_*`), which are protocol, not
  magic numbers.
- **Catalog integrity (beyond TD8-2026-08-12-02)** — both lists verified
  strictly sorted by `str::cmp` (required by `find`'s `binary_search_by`), zero
  exact duplicates, and the four FO4 case-collisions
  (`CloseMenu`/`closeMenu`, `GetButtonFromUserEvent`/`getButtonFromUserEvent`,
  `OnAcceptPress`/`onAcceptPress`, `PlaySound`/`playSound`) confirmed as
  *deliberate* and documented as such in `docs/engine/ui.md`. Skyrim's 12
  `Request` entries and FO4's zero are both correct per the protocol difference
  the catalog comments explain.
- **`crates/ui/` doc accuracy (beyond the three findings)** — `docs/engine/ui.md`'s
  module map matches `crates/ui/src/` file-for-file; its 74/138 counts, the
  129 + 9 = 138 derivation, the "16 default tests plus three ignored" claim, the
  `UiVertex` 20-byte figure, and the `ScaleformProfile` split all check out
  against code. `ROADMAP.md:627-628` and `docs/feature-matrix.md:161-170` agree
  with `catalog.rs` on both counts.
- **Dimension 3 (broader)** — the `classify_pbr` doc-rot trap named in the
  skill recipe is clean: `crates/core/src/ecs/components/material.rs` frames the
  deleted render-time entry point historically at every mention.
  `GpuCamera`/`GpuInstance`/`GpuMaterial` sizes are unchanged since 08-07 and
  correctly pinned; the only stale layout number found this sweep is `Vertex`
  (TD3-2026-08-12-01).

## Deferred

- **TD8-2026-08-12-01** (dead ABC constant-pool entries) is *sequenced*, not
  deferred: it is blocked on TD7-2026-08-12-01 landing first. Attempting it
  alone silently renumbers the pool.
- Nothing else is gated on an in-progress milestone. TD8-2026-08-12-03's
  full resolution (wiring `drain_calls` to real engine behavior) is M48 work
  by definition, but its **bounding** half is not and should not wait.

---

Report generated by `/audit-tech-debt` (ui-deep suite leg), 2026-08-12.
Publish with `/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-08-12.md`.
