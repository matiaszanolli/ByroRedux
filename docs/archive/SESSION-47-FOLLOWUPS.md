# Session 47 Follow-Up Plan — Haiku Execution Brief

> **Audience:** a less-capable model (Haiku 4.5) executing one work item (WI) at a time.
> **Source:** the four open threads left by Session 46 (commit `04b8d4d9`):
> R6a-stale-15 bench, ReSTIR-DI Phase 2, M24.2 Phase 2 follow-ups, M49 sub-items.
> Every file path is absolute-from-repo-root. Every line number was current at
> `04b8d4d9` — **re-grep the anchor symbol before editing; do not trust a bare
> line number.**

---

## How to use this document

1. Work items are grouped into **tiers by suitability for an autonomous small model.**
   Do the tiers **in order**. Inside a tier, WIs are independent unless a
   `Depends on:` line says otherwise.
2. Each WI has a fixed shape:
   - **Goal** — one sentence.
   - **Files** — exact paths + anchor symbols.
   - **Steps** — mechanical, numbered.
   - **Tests** — what to add + how to run.
   - **Done when** — acceptance checks (must all pass).
   - **Commit** — conventional-commit subject line.
   - **STOP-AND-ASK** — conditions under which you must NOT proceed and must
     hand back to the user / Opus instead.
3. **Global rules (non-negotiable, from CLAUDE.md + project memory):**
   - Run `cargo check` after every edit; `cargo test -p <crate>` before every commit.
   - **No `Co-Authored-By` / AI trailer** in commit messages — body only.
   - **Never guess a value or heuristic.** If a format, offset, or magic number
     is unknown, STOP-AND-ASK. (project memory: *No Guessing Policy*.)
   - **Never add per-game `if game == FNV` branches to the shader or renderer.**
     Translate at the parser→Material boundary. (memory: *Format Translation Layer*.)
   - Improve existing code; do not duplicate logic (global instruction).
   - One WI = one commit. Do not batch unrelated WIs.

---

## Tier 0 — Doc-rot & safe mechanical fixes (DO THESE FIRST)

These are bounded, low-risk, and build confidence. No new behavior.

### WI-0.1 — Fix stale M49 doc comments

**Goal:** the three code comments that still describe pre-M49 "no CSG / spawns zero
entities" state are wrong now that M49 shipped. Update them to current reality.

**Files (re-grep each before editing):**
- `byroredux/src/cell_loader/precombined.rs:11-31` — module header still says
  "spawns zero entities" (audit finding FO4-D9-DOC-01).
- `byroredux/src/cell_loader/load.rs:233-237` — stale "no CSG reader / we don't yet parse" comment (FO4-D9-DOC-02).
- `byroredux/src/cell_loader/exterior.rs:330-334` — same stale comment (FO4-D9-DOC-02).

**Steps:**
1. Read each comment block in full.
2. Rewrite to describe the **current** behavior: M49 closed; CSG `_oc.nif`
   precombined geometry is decoded and spawned; remaining deferred sub-items are
   `_precomb.nif` collision and `.uvd` occlusion (still open). Keep the deferred
   list at `precombined.rs:29-33` accurate.
3. Do **not** change any code — comments only.

**Tests:** none (comment-only). Run `cargo check` to confirm no accidental code edit.

**Done when:** `cargo check` clean; the three comments describe post-M49 reality;
the deferred-items list still names `_precomb.nif` collision + `.uvd`.

**Commit:** `docs(cell-loader): correct stale pre-M49 precombine comments (FO4-D9-DOC-01/02)`

**STOP-AND-ASK:** if the audit finding text (`docs/audits/AUDIT_FO4_2026-06-02.md:221-234`)
contradicts what the code actually does now — surface the discrepancy, don't paper over it.

---

## Tier 1 — M24.2 ESM decoding (clean Rust + synthetic-byte tests — IDEAL for Haiku)

This is the highest-value Haiku tier: pure data decoding, every change covered by
co-located synthetic-byte unit tests. Test pattern to copy: each parser file has a
`mod tests` with a local `fn sub(typ: &[u8;4], data: Vec<u8>) -> SubRecord` helper —
see `crates/plugin/src/esm/records/misc/magic.rs:558-1028` and
`crates/plugin/src/esm/records/misc/ai.rs:505-779`.

Run tests with: `cargo test -p byroredux-plugin esm::` (and the specific module name).

### WI-1.1 — Thread FormID remap through DIAL/INFO/AVIF parsers

**Goal:** `parse_dial` (QSTI), `parse_info` (TCLT/PNAM/ANAM), and `parse_avif`
(PNAM perk list) currently read FormIDs as **raw `u32` without `reader.remap_form_id`**.
Cross-plugin references therefore mis-resolve. Fix the plumbing.

**Depends on:** nothing. Do this first in Tier 1 — later WIs build on remapped IDs.

**Files:**
- `crates/plugin/src/esm/records/misc/ai.rs:346` (`parse_dial`), `:356` (QSTI raw read),
  `:369` (`parse_info`), `:335` (TCLT), `:338` (PNAM), `:343` (ANAM).
- `crates/plugin/src/esm/records/misc/effects.rs:44` (`parse_avif`), `:526` area (PNAM perks).
- `crates/plugin/src/esm/reader.rs` — find `remap_form_id` (grep for `fn remap_form_id`).
- `crates/plugin/src/esm/records/mod.rs:565` (DIAL dispatch), `:604` (AVIF dispatch) —
  to see what handle/context is available at the call site.

**Steps:**
1. **First, investigate the signature mismatch.** `parse_dial`/`parse_info`/`parse_avif`
   today take only `(form_id, subs)` — they have **no `EsmReader` handle**, so they
   *cannot* call `remap_form_id` as written. Look at how `parse_perk` and other
   parsers that DO remap get their context. Determine the minimal change:
   pass either `&EsmReader`, or a pre-built remap closure/table, into these parsers.
2. Mirror exactly what an already-remapping parser does (find one with
   `grep -rn remap_form_id crates/plugin/src/esm/records/`). Copy that pattern;
   do not invent a new one.
3. Apply `remap_form_id` to: QSTI, TCLT, PNAM, ANAM, and AVIF's PNAM perk list reads.
4. Update the dispatch call sites in `records/mod.rs` to pass the new argument.

**Tests:**
- Extend the existing DIAL/INFO unit tests in `ai.rs:505-779` and the AVIF test in
  `effects.rs:526` with a case where the input FormID has a non-zero plugin index and
  assert it is remapped (use a fake load-order/remap setup matching how existing
  remap tests are built — grep `remap` in the plugin test files for the pattern).
- Keep the existing `#[ignore]` real-data guard `parse_real_fnv_dial_infos_populated`
  (`crates/plugin/src/esm/records/tests.rs:192`) green.

**Done when:** `cargo test -p byroredux-plugin esm::` green; new remap assertions pass;
no parser reads QSTI/TCLT/PNAM/ANAM/AVIF-PNAM as bare `u32` anymore.

**Commit:** `fix(esm/dial): thread FormID remap through DIAL/INFO/AVIF parsers`

**STOP-AND-ASK:** if threading `&EsmReader` requires touching more than ~4 call sites
or changing a public signature used outside `esm/` — report the blast radius first.

---

### WI-1.2 — Per-entry-point EPFD depth for PERK

**Goal:** PERK entry-point function-data (EPFD) is decoded by keying off the EPFT
*formatter byte* only (`magic.rs:251-280`). The function_type→shape map
(1→None, 2→Float, 3→Range, 4→FormId, 5→LString) is the FO3/FNV formatter
convention, not the per-entry-point semantic. Add an entry-point-index → expected-shape
table as the authoritative decoder, with the formatter byte as a secondary hint.

**Depends on:** nothing (independent of WI-1.1).

**Files:**
- `crates/plugin/src/esm/records/misc/magic.rs` — `parse_perk` (`:145`), DATA per-entry
  (`:200-233`), EPFT (`:238-247`), EPFD (`:251-280`), EPF2/EPF3 raw capture (`:281-298`),
  `PerkFunctionData` enum (`:11-18`), `PerkEntryBody::EntryPoint` (`:51-68`).
- **Reference data (DO NOT GUESS):** the ~120-entry-point catalog is documented in
  project memory `perk_entry_points.md` (Papyrus/Creation reference). Read it for the
  per-entry-point canonical data shapes before writing the table.

**Steps:**
1. Read `perk_entry_points.md` and extract, for each entry-point index, its canonical
   function-data shape (None / Float / Range / FormId / LString / formatter-string).
2. Add a lookup table (a `const` slice or `match` keyed by `entry_point_index: u8`)
   that returns the expected `PerkFunctionData` variant.
3. In the EPFD arm, decode using the **entry-point-index table** as the source of
   truth; use `function_type` only to disambiguate where the catalog says a point is
   polymorphic. On mismatch, prefer the catalog shape and log a debug warning rather
   than silently producing `None` (current `magic.rs:277` swallows mismatches).
4. **Scope guard:** if `perk_entry_points.md` does not unambiguously specify a shape
   for an index, leave that index on the current formatter-byte path and note it
   `// TODO(EPFD): index N shape unverified` — do NOT guess.

**Tests:**
- Add unit tests in `magic.rs` `mod tests` (alongside `:831-913`) that drive EPFD by
  **entry_point_index** (not function_type) for at least: a Float point, a FormId
  point, a Range point, and one index where catalog-shape disagrees with the
  formatter byte (assert catalog wins).

**Done when:** `cargo test -p byroredux-plugin esm::records::misc::magic` green;
new index-driven tests pass; no index is decoded from a guessed shape.

**Commit:** `feat(esm/perk): per-entry-point EPFD shape table (M24.2 follow-up)`

**STOP-AND-ASK:** if `perk_entry_points.md` lacks the shape for a significant fraction
of indices — report coverage % and ask whether to ship partial + TODOs or wait for a
source. Also STOP if Skyrim+/FO4 extend `function_type` beyond the FO3/FNV `0x00..0x09`
range and a `GameKind` parameter would be needed (that's a bigger refactor — see note
at `magic.rs:34-35` and the `parse_perk` signature `:145` which takes no `GameKind`).

---

### WI-1.3 — DIAL conversation-tree resolution (PNAM ordering + TCLT edges)

**Goal:** INFOs are stored as a flat `Vec` per topic with `previous_info` (PNAM) and
`topic_links` (TCLT) captured but never resolved into a graph. Build a resolved
conversation structure: PNAM back-pointers ordered into per-topic chains, TCLT as
inter-topic edges.

**Depends on:** WI-1.1 (needs remapped PNAM/TCLT FormIDs to link correctly across plugins).

**Files:**
- `crates/plugin/src/esm/records/misc/ai.rs` — `DialRecord` (`:280-302`),
  `InfoRecord` (`:311-344`), `parse_dial` (`:346`), `parse_info` (`:369`).
- `crates/plugin/src/esm/records/grup_walker.rs:105-198` — `extract_dial_with_info`
  / `walk_info_records` (how INFOs attach to a DIAL today).
- `crates/plugin/src/esm/records/index.rs:111` — `dialogues` map.
- Integration test machinery: `crates/plugin/src/esm/records/tests.rs:9` (`build_record`),
  `:27` (`wrap_group`), `:92` (`dial_topic_children_walked_into_dialogue_infos`).

**Steps:**
1. Add a resolved type — e.g. `ConversationTree` or an ordering method on `DialRecord`
   — that, given `DialRecord.infos`, produces the INFOs ordered by their PNAM chain
   (PNAM == 0 is the chain head; each subsequent INFO points back to its predecessor).
2. Surface TCLT `topic_links` as explicit inter-topic edges (a map
   topic_form_id → Vec<linked_topic_form_id>), so a caller can walk topic→topic.
3. Build the resolution as a **pure function over already-parsed records** (no new
   byte parsing) so it is trivially unit-testable. Do not mutate the on-disk parse path.
4. **Scope guard — do NOT attempt** in this WI: Skyrim's LNAM/SNAM/DNAM link model,
   DLBR branches, DLVW quest views, SCHR/SCDA result scripts. Those are separate WIs
   (see WI-2.x). This WI is FO3/FNV PNAM+TCLT only.

**Tests:**
- Unit test the ordering helper in `ai.rs` `mod tests`: build 3 INFOs with a PNAM
  chain (A←B←C, A has PNAM=0) in scrambled `infos` order; assert resolved order is A,B,C.
- Integration test in `records/tests.rs` following `:92`: build a DIAL + Topic-Children
  GRUP with multiple INFOs and a TCLT edge; assert the tree links across topics.

**Done when:** `cargo test -p byroredux-plugin esm::` green; ordering + edge tests pass;
the existing `dial_topic_children_walked_into_dialogue_infos` test still passes.

**Commit:** `feat(esm/dial): resolve PNAM chains + TCLT edges into conversation tree`

**STOP-AND-ASK:** if the PNAM chain in real FNV data has cycles or multiple heads
(the resolver must not infinite-loop) — surface the data shape before picking a policy.

---

## Tier 2 — M24.2 Phase C schema layer (Rust refactor — medium difficulty)

This is a **refactor**, not new behavior. Higher risk than Tier 1 because it touches
shared infrastructure. Do Tier 1 first.

### WI-2.1 — Prototype a typed `read_sub::<T>` schema layer (ONE record first)

**Goal:** today every parser hand-writes `for sub in subs { match &sub.sub_type {
b"EDID" => …, … } }` with mixed direct-byte-indexing and `SubReader` cursor styles.
Phase C wants a typed `read_sub::<T>` that maps a 4-char code + field sequence to a
struct. **Prototype it on ONE simple record (ENCH or SPEL), prove it, stop.**

**Files:**
- `crates/plugin/src/esm/reader.rs:319` (`SubRecord`), `:472` (`read_sub_records`).
- `crates/plugin/src/esm/sub_reader.rs:57` (`SubReader`), strict reads `:118-207`,
  lenient `*_or_default` `:217-244`, demo tests `:372`, `:409`.
- `crates/plugin/src/esm/records/misc/magic.rs` — `parse_ench` (`:455` area),
  `parse_spel` (`:367` area) as the conversion target.

**Steps:**
1. Design a minimal trait, e.g.:
   ```rust
   trait SubRecordSchema: Sized {
       const CODE: [u8; 4];
       fn read(r: &mut SubReader) -> Result<Self>;
   }
   fn read_sub<T: SubRecordSchema>(sub: &SubRecord) -> Result<T> { ... }
   ```
   Build it on top of the **existing** `SubReader` strict/lenient readers — do not
   re-implement byte cursors.
2. Convert **only** ENCH's ENIT (or SPEL's SPIT) data sub-record to use it. Leave
   every other arm of that parser untouched.
3. Keep the public parser output identical — this is internal plumbing only.

**Tests:**
- Add a schema unit test in `sub_reader.rs` following the demo style at `:372`/`:409`.
- The existing ENCH/SPEL parser tests in `magic.rs` `mod tests` must pass **unchanged**
  (proves output parity).

**Done when:** `cargo test -p byroredux-plugin esm::` green; the converted record's
existing tests pass with no assertion changes; the trait lives in one place.

**Commit:** `refactor(esm): typed read_sub::<T> schema layer prototype (ENCH, Phase C)`

**STOP-AND-ASK:** **before** generalizing to more records. Phase C across all records
is a large design commitment — after the prototype, hand back to the user/Opus for a
go/no-go on rollout. Do NOT convert QUST/PERK/INFO (the block-structured parsers) —
their state machines don't fit a flat schema and need design review.

---

## Tier 3 — ReSTIR-DI Phase 2 (graphics — HIGH RISK, decompose + checkpoint)

> **Reality check.** Vulkan compute passes, descriptor layouts, shader/Rust
> lockstep, and resize wiring are exactly the class of change project memory flags as
> *Speculative Vulkan Fixes* — failures are invisible to `cargo test`. A small model
> should **build the scaffolding mechanically by copying the SVGF pipeline**, but
> **must hand each milestone to the user for RenderDoc/visual verification before
> proceeding.** Do NOT chain all of Tier 3 autonomously.

**The template you are copying everywhere:** `crates/renderer/src/vulkan/svgf.rs`
(full file) + `crates/renderer/shaders/svgf_temporal.comp`. SVGF already does
motion-vector reprojection + temporal accumulation + ping-pong history + the
two-phase counter handshake. ReSTIR temporal is structurally the same pipeline.

**Phase 1 (already shipped, `9abbe510`) — your inputs:**
- `Reservoir` GLSL struct: `crates/renderer/shaders/triangle.frag:1117-1127`
  (`{ uint lightIdx; uint M; float wSum; float W; }`, packed via `packReservoir` to `uvec4`).
- `RESERVOIR_FORMAT = R32G32B32A32_UINT`: `crates/renderer/src/vulkan/gbuffer.rs:60-63`.
- Per-pixel reservoir is **written to G-buffer color attachment index 6**
  (`outReservoir`, `triangle.frag:58`; export at `:3178-3184`; cleared to sentinel
  `0xFFFFFFFF` at `draw.rs:434-439`). **Nothing reads it back yet — that is Phase 2.**
- G-buffer attachment becomes `SHADER_READ_ONLY_OPTIMAL` at render-pass end
  (`context/helpers.rs:79`), so it is sample-ready immediately after `cmd_end_render_pass`.

### WI-3.1 — ReSTIR temporal resampling pass (scaffold by cloning SVGF)

**Goal:** add a compute pass that reads this-frame's reservoir image + last-frame's
reservoir history (reprojected via motion vectors), combines them with M-capping, and
writes a temporally-resampled reservoir history image.

**Files to create/edit:**
- **New shader:** `crates/renderer/shaders/restir_temporal.comp` — clone the
  reprojection + disocclusion logic from `svgf_temporal.comp` (motion read `prevUV = uv - motion`
  at `svgf_temporal.comp:114`; mesh_id disocclusion `:147-155`; normal-cone test `:164-165`).
- **New Rust pipeline:** `crates/renderer/src/vulkan/restir.rs` — clone the *structure*
  of `svgf.rs`: `partial`/`try_or_cleanup!` creation (`svgf.rs:288-318`), descriptor
  layout + `validate_set_layout` against reflection (`svgf.rs:372-451`), ping-pong
  `HistorySlot` per frame-in-flight (`svgf.rs:150-204`, `545-647`), read-prev/write-curr
  `prev = (f+1) % MAX_FRAMES_IN_FLIGHT` (`svgf.rs:658-659`), `dispatch` with pre/post
  barriers (`svgf.rs:830-929`), `upload_params` + `mark_frame_completed` counter
  handshake (`svgf.rs:781-818`, `938-951`), `recreate_on_resize` (`svgf.rs:965-1061`).
- **Construction wiring:** `crates/renderer/src/vulkan/context/mod.rs` — add field +
  `*_failed` latch near the SVGF field (`:1172`, `:1230`); construct after G-buffer
  views collected, passing `reservoir_views` (already gathered at `:1902-1903`) +
  motion/mesh_id/normal view slices; call `initialize_layouts` (mirror `:1929-1934`).
- **Resize wiring:** `crates/renderer/src/vulkan/context/resize.rs` — call
  `restir.recreate_on_resize(...)` next to the SVGF call (`:358-372`), passing the
  re-collected `reservoir_views` (`:633-634`); reset the failed latch near `:619`.
- **Dispatch slot:** `crates/renderer/src/vulkan/context/draw.rs` — insert the dispatch
  **immediately after `cmd_end_render_pass` (`:2711`) and before SVGF (`:2743`)**.
  Add the param-UBO upload alongside the other `upload_params` folds (`:1935-2110`,
  before the bulk HOST→COMPUTE barrier). Add `restir.mark_frame_completed()` next to the
  SVGF/TAA ones (`:3195-3200`).

**Design decisions you must NOT guess — STOP-AND-ASK if unspecified:**
- **History storage choice.** SVGF uses dedicated `STORAGE | SAMPLED` history images in
  `GENERAL` layout. The reservoir G-buffer attachment is `COLOR_ATTACHMENT | SAMPLED`
  only (`gbuffer.rs:103`), NOT storage. The clean path is **dedicated ReSTIR history
  slots** (mirror SVGF's `HistorySlot`), leaving G-buffer usage flags untouched. Use
  that path; do NOT add `STORAGE` to the G-buffer attachment without sign-off.
- **M-cap value** (temporal history clamp) — do not invent a number. If not specified
  in a ReSTIR reference, STOP-AND-ASK. (memory: *No Guessing Policy*.)

**Tests:**
- `cargo build -p byroredux-renderer` must compile (descriptor-layout reflection
  validation at `svgf.rs:441-451` will fail the build if Rust bindings drift from the
  `.comp` — that is your correctness net).
- Add any pure-helper unit tests mirroring SVGF's (`svgf.rs:1118-1226`,
  `next_svgf_temporal_alpha` / `should_force_history_reset`) for whatever pure
  reset-gate helper you add.

**Done when:** clean build; the pass is dispatched in the right slot; resize recreates
the new history without validation errors; **AND the user has visually confirmed via
RenderDoc / a bench-hold session that the temporal reservoir is populating** (you
cannot confirm this yourself).

**Commit:** `feat(renderer/restir): temporal resampling pass (ReSTIR-DI Phase 2a)`

**STOP-AND-ASK / CHECKPOINT:** after the pass compiles and dispatches, **stop and hand
to the user for visual verification before WI-3.2.** Do not implement spatial resampling
on top of an unverified temporal pass.

### WI-3.2 — ReSTIR spatial resampling pass + consumer wiring

**Depends on:** WI-3.1 verified by the user.

This WI involves a real design decision (how the resampled reservoir feeds back into
shading — either re-read in `triangle.frag` next frame, or a final shading compute
pass). **This is Opus/human design territory, not autonomous Haiku work.** Scaffold
only once the design is chosen and written down. Left intentionally under-specified
here — escalate for a design before starting.

---

## Tier 4 — Blocked on reverse-engineering (NOT autonomous Haiku work)

Both M49 sub-items require cracking an undocumented on-disk format. Per the No-Guessing
policy, a small model must **not** invent format layouts. These are listed for
completeness with the prep work that *is* safe.

### WI-4.1 — `_precomb.nif` collision (BLOCKED — format/naming unverified)

**Status from investigation:**
- Precombines currently get a **synthesized trimesh collider** automatically via the
  fallback gate at `byroredux/src/cell_loader/spawn.rs:1063-1070` (because they spawn
  as `RenderLayer::Architecture` with empty `collisions`). So this is an
  **accuracy/optimization** item, **not** "no collision at all."
- `_precomb.nif` on-disk **naming and format are unverified in this codebase.** No
  parser, no hook, no doc note — only a deferred-list mention at `precombined.rs:33`.

**Safe prep work a small model MAY do (no RE):**
1. **Verify the premise.** Confirm at the precombine spawn site
   (`precombined.rs:226-247`) that `base_layer` actually resolves to
   `Architecture` for bake artifacts (they pass `placement_form_id_pair=None`). If it
   does, the trimesh fallback already fires and the "walk through floors" audit claim
   (FO4-D6-INFO-02) is stale — **report that finding**, don't write code.
2. Do NOT attempt to load/parse `_precomb.nif` — STOP-AND-ASK for the format spec or a
   reference (e.g. the `xEdit`/`BSPackedCombined` community notes the user can supply).

**STOP-AND-ASK:** before writing any `_precomb.nif` reader.

### WI-4.2 — `.uvd` occlusion volumes (BLOCKED — greenfield, no spec)

**Status from investigation:**
- No `.uvd` parser, no occlusion/PVS system exists. Culling is **frustum-only**.
- `.uvd` is FO4 previs visibility (PVS) data; **format is undocumented** in-repo.
- The XCRI visibility-group ref tail is currently parsed-then-discarded
  (audit FO4-D9-DOC-03, `AUDIT_FO4_2026-06-02.md:226-229`) — it is the natural key,
  but there is no render-side or ECS-side culling hook to receive it yet.

**This is greenfield + RE. NOT autonomous Haiku work.** STOP-AND-ASK for a format spec
and an architecture decision (where a CPU coarse-cull stage plugs in) before any code.

---

## Tier 5 — R6a-stale-15 bench (CANNOT be run autonomously — prep + record only)

> **Hard constraint.** The bench needs a real NVIDIA RTX GPU with ray-tracing
> (`VK_KHR_ray_query` + acceleration structures) **and** on-disk game data. `xvfb`
> provides a display, not a GPU; software Vulkan (lavapipe) has no RT pipeline. A
> headless/CI runner therefore **cannot** produce the FPS/fence numbers. Only the dev
> workstation (RTX 4070 Ti) can. A small model PREPARES and RECORDS; the human RUNS.

### WI-5.1 — Prepare the R6a-stale-15 bench commands (Haiku-doable)

**Goal:** build + lay out the three exact bench invocations and the verification
one-liner so the human can paste-and-run.

**Steps:**
1. Build: `cargo build --release -p byroredux -p byro-dbg`.
2. Emit the three commands **with the correct CWD note** (bare `--bsa` names resolve
   against CWD — wrong CWD → near-empty scene + spurious ~1792 FPS). Exact arg shapes
   are in `ROADMAP.md:738-740`; use `--bench-frames 300`. For Prospector and MedTek,
   append `--bench-hold` so byro-dbg can attach for the `IsCollisionOnly` check.
   - **Prospector (FNV)** — CWD `.../Fallout New Vegas/Data`: `--esm FalloutNV.esm
     --cell GSProspectorSaloonInterior --bsa "Fallout - Meshes.bsa"
     --textures-bsa "Fallout - Textures.bsa" --textures-bsa "Fallout - Textures2.bsa"`.
   - **Whiterun (SkyrimSE)** — CWD `.../Skyrim Special Edition/Data`:
     `--esm Skyrim.esm --cell WhiterunBanneredMare --bsa "Skyrim - Meshes0.bsa"
     --bsa "Skyrim - Meshes1.bsa"` + **all** `Skyrim - Textures0.bsa … Textures8.bsa`
     (numeric-sibling auto-load does NOT fire on a digit suffix — list all 9).
   - **MedTek (FO4)** — CWD `.../Fallout 4/Data`: `--esm Fallout4.esm
     --cell MedTekResearch01 --bsa "Fallout4 - Meshes.ba2"
     --bsa "Fallout4 - MeshesExtra.ba2"` + `Textures1…Textures9.ba2` + `TexturesPatch.ba2`
     + `--materials-ba2 "Fallout4 - Materials.ba2"`.
3. Emit the verification one-liner (attach to a `--bench-hold` Prospector/MedTek run):
   `printf "ping\nentities\ntex.missing\nquit\n" | ./target/release/byro-dbg`
   — the `entities` output ends with `IsCollisionOnly (render+phys combined, expect 0): N`
   (`byroredux/src/commands.rs:96-129`). R6a-stale-15 part (b) requires `N == 0` on
   Prospector and MedTek.
4. Capture the `bench:` summary line from **stdout** (the engine does NOT write it to a
   file — `main.rs:2143-2228`). FPS = `wall_fps`; fence = the `fence=` bracket.

**Done when:** the build succeeds and a copy-paste-ready command block + the two
acceptance checks (fence vs target, `IsCollisionOnly=0`) are presented to the user.

**STOP:** you cannot run the bench. Present the commands and **ask the user to run them
and paste back the three `bench:` lines + the two `entities` IsCollisionOnly counts.**

### WI-5.2 — Record R6a-stale-15 results (after the user pastes numbers back)

**Goal:** once the user supplies the numbers, update the bench-of-record.

**Steps:**
1. Flip `ROADMAP.md:676` `- [ ] **R6a-stale-15**` → `- [x]`, write the closeout prose
   (HEAD commit, commit-delta, per-scene FPS/fence/entity, interpretation), following
   the exact style of the R6a-stale-14 entry just above it.
2. Advance the bench-of-record table (`ROADMAP.md:16-29`) and the repro table
   (`ROADMAP.md:738-740`) to the new HEAD commit + new numbers.
3. Verify the two acceptance criteria in prose: (a) fence recovered beyond
   R6a-stale-14's 11.12 ms toward the pre-collider 2.62 ms @ ~2564 ent;
   (b) `IsCollisionOnly == 0` on Prospector and MedTek.
4. Add a HISTORY.md entry (or fold into the next `/session-close`).

**Done when:** ROADMAP bench-of-record + repro table + checklist all reflect the new
numbers and agree with each other; HISTORY records it.

**Commit:** `docs(bench): close R6a-stale-15 — fence recovery confirmed at <HEAD>`

**STOP-AND-ASK:** if the user's numbers show fence did **not** recover (still ~11 ms) or
`IsCollisionOnly != 0` — do not close the item; report the regression and file the
follow-up instead.

---

## Suitability summary (read this before picking work)

| Tier | Thread | Haiku-autonomous? | Why |
|------|--------|-------------------|-----|
| 0 | M49 doc-rot fixes | ✅ Yes | Comment-only, bounded |
| 1 | M24.2 remap / EPFD / DIAL tree | ✅ Yes | Pure Rust + synthetic-byte tests |
| 2 | M24.2 Phase C prototype | ⚠️ One record then STOP | Refactor; rollout needs design sign-off |
| 3 | ReSTIR-DI Phase 2 | ⚠️ Scaffold only, checkpoint each pass | Graphics; failures invisible to `cargo test`, needs RenderDoc |
| 4 | M49 `_precomb.nif` / `.uvd` | ❌ No | Undocumented formats — No-Guessing policy blocks RE |
| 5 | R6a-stale-15 bench | ❌ Run; ✅ prep+record | Needs RTX GPU + game data |

**Recommended Haiku run order:** WI-0.1 → WI-1.1 → WI-1.2 → WI-1.3 → WI-2.1 (stop) →
WI-5.1 (prep, hand off). Tiers 3 and 4 require a human/Opus checkpoint before starting.
