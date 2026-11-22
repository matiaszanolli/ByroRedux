# #7004: UI-D3-2026-11-15-01: catalog.rs and its test comments still describe the retired 76/66 provenance split (recurrence of closed #4721)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/7004
- **Labels**: low,ui,game:skyrim,documentation,doc-rot
- **Source**: docs/audits/AUDIT_UI_2026-11-15.md

From the UI audit `docs/audits/AUDIT_UI_2026-11-15.md` (HEAD 8473ae994).

- **Severity**: LOW
- **Dimension**: Catalog & AVM1 Scanner
- **Profile**: SkyrimAvm1
- **Existing**: none open. This is the class of closed #4721 (stale catalog size and shape after the #3103 regeneration), reintroduced by #5274's all-`Measured` promotion, which fixed `ui.md`, the module comment and the guards but not these sites.
- **Location**:
  - `crates/ui/src/catalog.rs:22-29` (the `Measured` variant doc)
  - `crates/ui/src/catalog.rs:85-87` (`command_heuristic` doc)
  - `crates/ui/src/catalog.rs:97-99` (`request_heuristic` doc)
  - `crates/ui/src/avm1_host/tests.rs:364-367`
  - `crates/ui/src/avm1_host/tests.rs:419-424`
  - `crates/ui/src/avm1_host/tests.rs:418-433` (duplicate block)
  - `crates/ui/src/host/tests.rs:283-290`
- **Evidence** (all re-read at HEAD):
  - `catalog.rs:22-29` says "Skyrim's original 74 entries ... (commands plus 12 requests, joined by #4720's two corpus-promoted requests ...), and the 66 #3103 corpus-sweep entries". The actual partition is 74 SkyUI-sourced + 68 sweep entries (`catalog.rs:719-722`, `:739`, `:179`, `:203`, and `ui.md:609-612` all say 68). The two #4720 requests (`LoadDLC`, `RequestLoadingText`) are two of the 68, so the sentence builds the pre-#5274 76/66 split. Requests are 12 SkyUI-sourced + 4 sweep-sourced (`LoadDLC`, `RequestLoadingText`, `GetMouseButtonForSetDestination`, `ShouldShowMod`) = 16, as `ui.md:612-617` states.
  - `catalog.rs:85-87` says `command_heuristic` is used for "#3103's 66 Skyrim entries", and `:97-99` says `request_heuristic` is used for "#3103's 2 remaining Skyrim entries". Both are false: no Skyrim entry uses a heuristic constructor (script count: Skyrim `command` 126 / `request` 16, zero `*_heuristic`). `skyrim_catalog_is_fully_measured` pins that.
  - `avm1_host/tests.rs:364-367`: "the rest keep `HeuristicNamePrefix`, and `skyrim_catalog_provenance_split_matches_the_3103_sweep` pins the 76/66 split". That test does not exist (`git grep` finds no definition) and the rest do not keep the heuristic.
  - `avm1_host/tests.rs:419-424`: the failure message partitions the 16 requests as "command 62 / request 14 / sweep-command 64 / sweep-request 2". The totals add up, but the partition is the old 76/66 basis. At HEAD it is SkyUI 62 commands + 12 requests, sweep 64 commands + 4 requests.
  - `host/tests.rs:283-290`: "14 from SkyUI's fourth-argument rule (12 original + the two #4720 promotions) plus the 2 of the sweep's remaining 66 that the name-prefix heuristic classifies as queries". The assert value 16 is right, but the heuristic story is gone.
  - `avm1_host/tests.rs:418-433`: the assertion `found.methods.len() >= 141` with its message is written out twice in a row (a copy/paste duplicate, no effect on behaviour).
- **Impact**: no behaviour change. A maintainer reading the `Measured` doc is told the wrong provenance counts and that heuristic Skyrim constructors are in use, which is exactly the "stray 66/76 split" the skill calls doc rot. It also points at a pin test that is not there.
- **Suggested Fix**: rewrite the `Measured` doc as "74 SkyUI-sourced entries (SkyUI's fourth-argument rule, 12 requests) + 68 sweep entries typed by measured per-site arity (#4721/#5274; 4 requests: `LoadDLC`, `RequestLoadingText` #4720, `GetMouseButtonForSetDestination`, `ShouldShowMod` #5274)". Say `command_heuristic` / `request_heuristic` are FO4-only (#2966's 131). Fix the two test comments and the failure message partition, point `tests.rs:366` at `skyrim_catalog_is_fully_measured`, and delete the duplicated assert block. Add a text-scan guard as in the "Refactor vs Text-Scan Guards" convention only if desired; `skyrim_catalog_is_fully_measured` already pins the code.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
