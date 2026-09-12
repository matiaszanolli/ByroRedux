# SpeedTree Subsystem Audit — 2026-09-11

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker (`parser.rs`, `stream.rs`, `tag.rs`, `version.rs`, `scene.rs`) and the
placeholder-billboard importer (`crates/spt/src/import/mod.rs`) — plus the
cross-cut wiring: `byroredux/src/cell_loader/references/synth_child.rs`,
`byroredux/src/cell_loader/references/import.rs`,
`byroredux/src/cell_loader/nif_import_registry.rs`,
`byroredux/src/cell_loader/spawn.rs` + `spawn/mesh_instance.rs`,
`byroredux/src/scene/nif_loader.rs`, `crates/plugin/src/esm/records/tree.rs`,
`byroredux/src/systems/billboard.rs`, `crates/spt/docs/format-notes.md`,
`docs/engine/exal-trees.md`.

**Execution**: single-pass, solo, per this skill's own "small enough to run
inline" instruction — no sub-agents dispatched.

**Depth**: `shallow` (source + prior-report diff; no on-disk BSA corpus
re-run this cycle — `cargo test -p byroredux-spt --lib` re-run instead, see
below).

**Method**: read the most recent prior report
(`AUDIT_SPEEDTREE_2026-08-30.md`), verified each of its five findings against
current `git log`/`git show` for the fix commit, then walked all six
dimensions against current source, with special attention to the SKILL's own
flagged open item (the `#3808` tail-desync gap) and to whether the two
`Fix "doc rot"` commits (`57fdcc57`, `b214d0ae`) actually swept every
occurrence of the claims they corrected.

- `cargo test -p byroredux-spt --lib` → **54/54 pass**, 0 failed (48→51→54
  across the last three cycles; consistent with three guards added by
  `#3529`/`#3531` plus the `#3808` marker-value pins in `scene.rs`).
- No BSA corpus re-run this cycle (shallow depth) — the 2026-08-30 report's
  100%/100%/96.46% acceptance numbers and the 2026-09-07 `#3808` spike's
  159-file desync measurement are taken as current; nothing in the diffed
  commit window touches the walker's tag dictionary or dispatch table in a
  way that would change either.

---

## Dedup pass (mandatory)

Fresh `gh issue list --repo matiaszanolli/ByroRedux --search "speedtree OR spt
OR TREE"` this run → `/tmp/audit/speedtree/issues.json`. All five findings
from `AUDIT_SPEEDTREE_2026-08-30.md` map to closed issues with fixes verified
in place at HEAD:

| Issue | 08-30 finding | Fix commit | Verified at HEAD |
|---|---|---|---|
| #3735 | D3-01 (MODL resolution, HIGH) | `866ae87b` | `resolve_spt_model_path` + `SPT_CANDIDATE_DIRS` in place, `has_mesh_exact`/`extract_mesh_exact` wired, corpus gate exists |
| #3750 | D3-02 (cache key, LOW) | `daae655a` | `spt_cache_key` suffixes TREE form id, tests pin distinct-vs-same-record behaviour |
| #3751 | D3-03 (CNAM 5-vs-8 split, LOW) | `57fdcc57` | Fixed in `tree.rs` and `crates/spt/src/import/mod.rs` — **but see SPT-2026-09-11-D3-01 below: a third occurrence was missed** |
| #3752 | D1-01 (5 fatal conditions, LOW) | `57fdcc57` | `parser.rs`'s docstring now enumerates all five |
| #3740 | D4-01 (BNAM presence, MEDIUM) | `57fdcc57` | Fixed in `crates/spt/src/import/mod.rs` — **but see SPT-2026-09-11-D3-01 below: the same third file was missed** |

No open issue in the search covers either finding reported below.
`gh issue list --search "geometry-tail decoder"` and `--search "5-float
Oblivion"` both return only already-closed issues, none of which cover the
specific residual locations found this cycle.

---

## Findings

### SPT-2026-09-11-D3-01: `byroredux/src/cell_loader/references/import.rs` still asserts the two corpus-premise errors `#3740`/`#3751` fixed everywhere else

- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Location**: `byroredux/src/cell_loader/references/import.rs:509-529`
- **Status**: NEW (residual of closed #3740, #3751 — the fix commit
  `57fdcc57` swept `crates/plugin/src/esm/records/tree.rs` and
  `crates/spt/src/import/mod.rs` but not this file)
- **Description**: `57fdcc57` ("Fix doc rot: SpeedTree billboard wiring,
  CNAM/BNAM corpus claims") corrected two false corpus premises — "CNAM is 5
  floats on Oblivion, 8 on FO3/FNV" and "BNAM is FO3/FNV-only, absent on
  Oblivion" — in `crates/plugin/src/esm/records/tree.rs` and
  `crates/spt/src/import/mod.rs`. `byroredux/src/cell_loader/references/
  import.rs`, which builds the same `SptImportParams` from the same
  `TreeRecord` fields one call site further out, states both false premises
  verbatim and was not touched by that commit:
  - Line 509-510: *"CNAM's positional semantics remain unpinned across the
    5-float Oblivion and 8-float Fallout layouts."* — CNAM is 8 floats on
    all three games (142/142 Oblivion, 9/9 FO3, 3/3 FNV measured, `#3751`);
    there is no split.
  - Line 517-521: *"Oblivion ships MODB on 100% of TREE records and OBND on
    none, so the placeholder size fallback needs MODB to size Cyrodiil trees
    correctly"* — the OBND/MODB percentages are correct, but the conclusion
    is exactly the disproved `#3740`/D4-01 premise: Oblivion also ships BNAM
    on 100% of `.spt`-bearing TREE records, BNAM outranks MODB in
    `compute_billboard_size`'s precedence, so BNAM — not MODB — actually
    sizes every vanilla Oblivion tree.
  - Line 524-529: *"#1002 — BNAM (FO3/FNV billboard width × height) as a
    fallback BELOW OBND"* — restates BNAM as FO3/FNV-only, the same false
    premise a second time in the same comment block.
- **Evidence**:
  ```
  509:    // CNAM's positional semantics remain unpinned across the 5-float
  510:    // Oblivion and 8-float Fallout layouts. Do not invent named wind fields
  517:    // #1001 — Oblivion ships MODB on 100 % of TREE records and OBND
  518:    // on none, so the placeholder size fallback needs MODB to size
  519:    // Cyrodiil trees correctly (vanilla MODB range 157–3621 game
  520:    // units). FO3/FNV are inverse: 100 % OBND, 0 % MODB. Surface both
  521:    // and let `compute_billboard_size` pick its precedence.
  524:    // #1002 — BNAM (FO3/FNV billboard width × height) as a fallback
  525:    // BELOW OBND. Corpus inspection (2026-05-13) showed BNAM clamps
  526:    // tall trees vs their physical OBND extent (e.g. `WhiteOak01`
  527:    // BNAM 768×768 vs OBND 802×1567), so OBND wins for the
  528:    // whole-tree placeholder. BNAM only reaches `compute_billboard_size`
  529:    // when OBND is absent — a rare mod-content case in FO3/FNV.
  ```
  A repo-wide grep for the CNAM split phrasing after the fix commit turns up
  exactly one remaining hit — this file:
  ```
  crates/spt/src/import/mod.rs:78:  ...8 × f32 on all three games — Oblivion, FO3, FNV, no split)   [FIXED]
  crates/plugin/src/esm/records/tree.rs:194: ...8 × f32 on all three games... no split.               [FIXED]
  byroredux/src/cell_loader/references/import.rs:509-510: ...5-float Oblivion and 8-float Fallout...  [STALE]
  ```
- **Impact**: No functional consequence — the actual code at this call site
  (lines 522, 530) correctly reads `t.bound_radius` and `t.billboard_size`
  and passes both through `SptImportParams` unconditionally, so the
  precedence bug D4-01 already fixed in behaviour is not reintroduced. The
  cost is purely that a future editor reading this specific call site (the
  one that actually assembles `SptImportParams` from a live `TreeRecord`,
  arguably the most load-bearing comment block in the wiring for
  understanding *why* the fields are threaded the way they are) will
  reason from both disproved premises, exactly the failure mode `#3740` and
  `#3751` were filed to close.
- **Related**: #3740, #3751 (both closed, both partially — not regressed in
  behaviour, only in documentation completeness at this one additional
  site); D4-01 and D3-03 in `AUDIT_SPEEDTREE_2026-08-30.md`.
- **Suggested Fix**: Replace lines 509-510 with the measured 8-floats-on-all-
  three-games fact (mirroring the wording now in `import/mod.rs`), and
  replace 517-529 with a corrected statement that BNAM, not MODB, is the
  Oblivion-reachable tier (mirroring D4-01's fix in `import/mod.rs`'s
  `bound_radius` field doc). No behaviour change — this is a comment-only
  fix — but ideally landed alongside any future edit to this function so the
  claim doesn't get copy-pasted a fourth time.

---

### SPT-2026-09-11-D1-01: three source files still describe SpeedTree geometry as living in a deferred "geometry tail" that `#3808` (2026-09-07) determined does not exist

- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting / Placeholder Fallback (doc-rot,
  spans both)
- **Location**: `crates/spt/src/parser.rs:1-25` (module doc); `crates/spt/
  src/import/mod.rs:9-48` (module doc, "SpeedTree Phase 2" section);
  `byroredux/src/cell_loader/references/import.rs:432-437,455-456,556-557`
- **Status**: NEW (residual of `#3808`, closed 2026-09-07 by `75ea6533` —
  that commit corrected `crates/spt/src/scene.rs` and `docs/engine/
  exal-trees.md` but did not sweep the source-code doc comments in the other
  files that describe the same now-superseded framing)
- **Description**: `#3808`'s research spike (see `crates/spt/docs/
  format-notes.md`, "2026-09-07 — Phase 2.1 geometry-tail dissection")
  established that what the walker calls `tail_offset` is not the start of a
  binary geometry section: it is a desync point inside the *same* TLV
  parameter stream, past `parser::TAG_MAX`, and no `.spt` in the 159-file
  corpus is large enough to hold baked branch/frond/leaf-card geometry at
  all (largest file 8,793 B, under the cost of 274 vertices of
  position+normal+UV for the *entire* file). `crates/spt/src/scene.rs`'s
  `tail_offset` doc and `docs/engine/exal-trees.md` were corrected to say
  so. Three other locations that make the identical claim were not:
  - `crates/spt/src/parser.rs`'s **module-level doc comment** (lines 1-25,
    predating and unchanged by `#3808`) still reads: *"Stops cleanly when
    the next tag is out of range — that's the binary geometry tail (Phase
    1.3 follow-up)"* and *"The peeked u32 isn't in `[TAG_MIN, TAG_MAX]` —
    geometry tail"* — describing the exact claim `scene.rs`, twelve lines
    away in the same crate, now explicitly retracts.
  - `crates/spt/src/import/mod.rs`'s **module-level doc comment** (lines
    9-48) still has a whole section, *"SpeedTree Phase 2 (planned, no
    ROADMAP row — gated by `crates/spt/docs/format-notes.md`'s 'Geometry
    tail' section)"*, whose first bullet is *"Decode the geometry tail past
    `tail_offset` → real branch / frond meshes with the bark texture"* — the
    literal thing `#3808` found is not possible to do because there is no
    geometry there.
  - `byroredux/src/cell_loader/references/import.rs` has three separate
    comments carrying the same premise: line 434 ("*When the geometry-tail
    decoder lands later, `byroredux_spt::import_spt_scene` will start
    producing real branch / frond meshes*"), line 455-456 ("*gives the Phase
    2 geometry-tail decoder a corpus trail*"), and line 556-557 ("*Real
    branch geometry might emit a sphere collision (tree-trunk collider) once
    the geometry tail is decoded*").
- **Evidence**: `scene.rs`'s own corrected doc explicitly names this exact
  failure mode: *"`tail_offset` used to be documented as 'where the binary
  geometry tail begins'. The 2026-09-07 dissection (#3808) measured that
  claim and it does not hold"* (`scene.rs:8-10`). The three locations above
  are precisely instances of the documentation `scene.rs` is contrasting
  itself against, still standing.
- **Impact**: None today — no code branches on "is this the geometry tail",
  the placeholder importer is unconditional either way, and nothing has
  attempted to write a Phase-2 decoder against these comments yet. The risk
  is forward-looking and specific: `format-notes.md`'s own 2026-09-07 entry
  says the *next* concrete parser task is "raising `TAG_MAX` and
  dictionarying the 14000-22000 bands... with the desync [SPT-2026-09-11-D1-02
  below] fixed first" — ordinary TLV work, not geometry decoding. A
  contributor who starts from `import/mod.rs`'s "Phase 2" section or
  `import.rs`'s three comments instead of `format-notes.md` would look for a
  geometry layout that the same audit cycle's own research proved does not
  exist, in the exact subsystem this report's own skill file was written to
  warn about.
- **Related**: #3808 (closed, correctly, but the sweep was scoped to
  `scene.rs` + `exal-trees.md` only); the audit-speedtree SKILL.md's own
  intro carries the corrected framing and explicitly says it "supersedes
  every earlier 'geometry tail'/'binary geometry tail' framing" — these three
  locations are exactly what that sentence should have swept in-repo but
  didn't.
- **Suggested Fix**: Reword `parser.rs`'s module doc to match `scene.rs`'s
  (a TLV stream that continues past `tail_offset`, capped by `TAG_MAX`, not
  a distinct geometry section). Retarget `import/mod.rs`'s "Phase 2" section
  at the three actual re-scoped directions `exal-trees.md` §3/§10 now lists
  (generate branch/leaf geometry from authored parameters / keep billboards
  permanently / source real tree meshes elsewhere) instead of "decode the
  geometry tail". Update the three `import.rs` comments similarly. All four
  are comment-only changes.

---

### SPT-2026-09-11-D1-02: the `#3808`-measured 46%-of-corpus `tail_offset` desync remains unfixed and untracked by any open issue

- **Severity**: MEDIUM
- **Dimension**: Walker Byte-Accounting
- **Location**: `crates/spt/src/parser.rs:35-38` (`TAG_MIN`/`TAG_MAX`),
  `:66-76` (tail-detection loop); `crates/spt/docs/format-notes.md:763-778`
  ("`tail_offset` is a desync point, not a section boundary")
- **Status**: NEW (the underlying measurement is not new — it is `#3808`'s
  own finding, recorded in `format-notes.md` on 2026-09-07 and repeated in
  this skill's own text as an explicit "do not report as closed" open
  item — but no GitHub issue tracks the fix, and `#3808`, the issue that
  produced the measurement, is itself CLOSED)
- **Description**: `#3808`'s corpus dissection measured, for each of 159
  `.spt` files across FNV/FO3/Oblivion+Shivering Isles, the byte shift from
  `tail_offset` that maximises known-tag hits against the existing
  dictionary: 86 files need 0, 36 need 1, 33 need 2, 4 need 3. That means in
  73 of 159 files (46%) the walker's stopping point is not a real boundary —
  it stopped *inside* a payload it had mis-sized, and one or more tags
  immediately before `tail_offset` consumed the wrong number of bytes. This
  is squarely Dimension 1's own core risk ("one mis-sized payload desyncs
  the whole stream"), now confirmed to actually be happening on nearly half
  the corpus, not hypothetical. `#3808` — the issue this measurement is
  filed under — is CLOSED, with its own body's "Status" line describing it
  as a completed research spike ("Phase 2.1... genuinely unstarted [before
  this]"); no separate follow-up issue exists for the fix itself.
- **Evidence**: `format-notes.md:763-778`:
  ```
  | shift | files |
  |---:|---:|
  | 0 | 86 |
  | 1 | 36 |
  | 2 | 33 |
  | 3 | 4 |
  ```
  Confirmed via `gh issue list --search "TAG_MAX OR desync OR resync OR
  mis-sized OR dictionarying"` and `--search "spt_tail OR exal-trees"` —
  neither returns an open issue naming this fix; `#3808` is the sole match
  and is closed.
- **Impact**: Bounded today for the identical reason the `#3808` entry
  itself gives: nothing consumes bytes past `tail_offset`, so a walker that
  stops 1-3 bytes into the wrong place is invisible to every current
  consumer (the placeholder importer, the acceptance-gate harness, the tag
  dictionary). The impact is entirely on the next piece of work this exact
  report's SKILL.md flags as "routine" — raising `TAG_MAX` and dictionarying
  the 14000-22000 tag bands `#3808` found recurring in ~151/159 files. That
  work cannot safely start from the current `tail_offset` on 46% of the
  corpus: extending the walker past a stop it doesn't know is wrong would
  silently misparse the newly-dictionaried tags in nearly half of all
  vanilla content, the same failure class Dimension 1 exists to catch.
- **Related**: #3808 (closed — the measurement, not the fix); this report's
  own skill file explicitly names this as "OPEN GAP, not yet fixed" and
  instructs "Do not report this as closed."
- **Suggested Fix**: File a dedicated follow-up issue for the desync fix
  itself (distinct from `#3808`, which only diagnosed it), scoped to: (1)
  identify which dictionary entry near each file's `tail_offset` is
  mis-sized — the recon tooling (`spt_tail` example) already computes the
  resync shift per file, so pairing that with the *last* decoded tag before
  `tail_offset` should localise the culprit tag(s); (2) fix the size in
  `tag.rs`'s dictionary; (3) re-run the acceptance gate to confirm the
  0-shift count rises from 86/159 toward 159/159 before attempting to raise
  `TAG_MAX`.

---

## Dimension summary (every dimension enumerated)

| Dimension | Findings | Notes |
|---|---:|---|
| 1 — Walker Byte-Accounting | **1** (MEDIUM) | `parser.rs`/`stream.rs`/`tag.rs` re-read in full; all `SptTagKind` arms still advance exactly their claimed width, both 64 KiB caps still bound byte count, LE-only unconditional reads confirmed, `#3531`'s two guards (`tag_13005_before_zero_leading_tail_resolves_as_bare`, `empty_candidate_is_not_a_plausible_curve_string`) present and passing. The finding is the `#3808`-measured 46% desync, which the skill itself flags as open and non-closable this cycle — carried forward, not newly discovered. (The parser.rs doc-rot half of what would otherwise be a Dim-1 finding is folded into D1-01 above, which spans Dims 1 and 2.) |
| 2 — Placeholder Fallback | **0** | `import_spt_scene` still has no `Err` path; leaf-texture precedence, `-Z` normals/`[0,3,2,2,1,0]` winding, `bs_bound` Z-up→Y-up, cutout alpha fields, and `clamp_billboard_extent`'s `Option`-returning NaN guard on all three tiers are all intact with their guards (spot-checked against the 08-30 report's evidence, re-read the current source, no drift). |
| 3 — TREE→Billboard Wiring | **1** (LOW) | `resolve_spt_model_path`/`SPT_CANDIDATE_DIRS` (#3735) and `spt_cache_key` (#3750) both verified in place and correctly wired through `synth_child.rs` and `import.rs`. `placement_root_billboard` confirmed still structurally `None` (the documented dead seam, #3533) with the live attach at `mesh_instance.rs`. The one finding is the residual doc-rot in `import.rs` that #3740/#3751's fix commit missed. |
| 4 — Per-Game Variants & Route Divergence | **0** | `version.rs` unchanged, `MAGIC_HEAD` exact-20-byte rejection confirmed, `detect_variant` still diagnostic-only with zero downstream branching. Both routes (`import.rs`'s cell-loader path, `nif_loader.rs`'s loose `--tree` path) call `parse_spt` + `import_spt_scene`; the documented `SptImportParams::default()` gap on the loose route stands as understood, not a new finding. |
| 5 — Tag Dictionary | **0** | `tag.rs` unchanged; 12002/12003 remain correctly flagged size-only with no corpus evidence per `#3535`'s fix, cross-checked against `format-notes.md`. No new confounders found. |
| 6 — NIFAL Material Translation | **0** | Single boundary holds — spot-checked `placeholder_billboard_mesh`'s `metalness_override`/`roughness_override`/`emissive_source` defaults and the two `translate_material` call sites; no drift from the 08-30 report's findings. |

**Totals**: 6 dimensions, **3 findings** — 0 CRITICAL, 0 HIGH, **1 MEDIUM**,
**2 LOW**. Dimensions **2, 4, 5 and 6 produced no findings.**

---

## Summary

Every finding from `AUDIT_SPEEDTREE_2026-08-30.md` is fixed and verified in
place: the MODL resolution gate (#3735, the prior cycle's HIGH) is real code,
not just a comment, the cache key carries the TREE form id (#3750), and the
tests (54/54, up from 51/51) confirm no regression. Separately, `#3808`'s
2026-09-07 research spike settled the long-open "confirm the geometry tail"
question — decisively, and in the negative: there is no baked geometry in any
`.spt` in the 159-file corpus, so Phase 2.2-2.4 (branch/frond/leaf-card
import) have been re-scoped rather than scheduled, per `exal-trees.md`.

What this cycle found is that both of those fixes were incomplete in the same
specific way: each landed in some but not all of the files carrying the claim
it corrected. `byroredux/src/cell_loader/references/import.rs` — the file
that actually assembles `SptImportParams` from a live `TreeRecord`, arguably
the single most load-bearing comment block for understanding the wiring —
still asserts the CNAM 5-vs-8 float split and the BNAM-absent-on-Oblivion
premise `#3740`/`#3751` disproved elsewhere, and three files still describe a
"geometry-tail decoder" as future work when `#3808` determined that decoder
has nothing to decode. Neither has any functional consequence today — the
actual code paths are correct — but both are exactly the kind of doc-rot this
subsystem has repeatedly shown costs real audit and engineering time once
someone reasons from the wrong premise (as `#3740`/`#3751`/`#3808` themselves
each were, in their own way, corrections of an earlier such reasoning error).

The one finding with a live, if currently invisible, engineering cost is the
carried-forward Dimension 1 item: `#3808` also measured that the walker's
`tail_offset` is a desync point on 46% of the corpus, not a clean boundary —
and that measurement has no fix and no tracking issue of its own, only the
now-closed research-spike issue that produced it. It is the stated
precondition for the "routine" work of extending `TAG_MAX`, and the skill
file for this audit explicitly instructs future auditors not to let it read
as closed.

### Suggested next step

`/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-09-11.md`

Labels: `speedtree` + `terrain-exterior` + `doc-rot` on D3-01 and D1-01;
`speedtree` + `terrain-exterior` on D1-02 (the desync gap is a real parser
defect, not documentation). Game labels: `game:oblivion` on D3-01 (the
BNAM/MODB half); no game label on D1-01 or D1-02 (both cross all three
`.spt` games). Types: `documentation` for D3-01 and D1-01; `bug` for D1-02.

Fix order: all three are independent and low-urgency. D1-02 (file a tracking
issue, even before attempting the fix) is the one with a forward engineering
cost if left silent, since it is the one item this report's own skill file
says must not be allowed to read as resolved.
