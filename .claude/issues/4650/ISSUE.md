# PAR-D1-2026-09-21-04: MenuXml <include> splices expand exponentially: include nesting never counts toward the depth cap

Labels: high,bug,ui,game:oblivion,game:fo3,game:fnv

## Description
`crates/menuxml/src/parse.rs:638-679` (`splice_include`) re-enters `parse_element_content` (`:600-611` call site) with the **same** `depth` value it received. The tile cap (48) and op cap (64) therefore never see include nesting at all.

- `seen_includes` is an ancestor-path stack. It stops a file from including itself on the current path (`include_cycles_terminate` covers this), but a DAG of distinct fragments with fan-out f and depth d is still expanded to f^d splices — no cap exists for that shape.
- Each splice re-fetches the fragment through `MenuFileSource::menu_xml`, which is an archive extract plus inflate in production.
- Spellings are compared as normalised strings (`x.xml`, `prefabs\x.xml`, `menus\prefabs\x.xml` all resolve to the same archive entry through the candidate list). A fragment that includes itself under K spellings therefore re-enters about K! times.

Verified unchanged at HEAD `ee6d3fb39`: `splice_include` still calls `parse_element_content(&mut scanner, src, seen_includes, depth)` — `depth`, not `depth + 1`.

## Evidence
Probe `menuxml-bomb` (in-memory source; each extra level doubles both time and memory):

```
depth 10 (11 files,  481 B):     1,025 tiles,     2,047 fetches,  10 ms
depth 17 (18 files,  817 B):   131,073 tiles,   262,143 fetches, 0.77 s, VmHWM 45.5 MB
depth 19 (20 files,  913 B):   524,289 tiles, 1,048,575 fetches, 3.09 s, VmHWM 177.5 MB
```

Depth 30 is about 1.4 KB of XML. It extrapolates to about 1.07e9 tiles, roughly 360 GB, and hours of archive fetches.

## Impact
Hang, then OOM abort, while loading the HUD.

- The path is `hud.rs` `launch_hud` -> `MenuRenderer::load_with_profile` / `graft_fragment`, on the main thread with no `catch_unwind`.
- Source: the single `Oblivion - Misc.bsa` / `Fallout - Misc.bsa` beside the ESM (`hud.rs:189-214`, opt-in `--hud`).
- Mods cannot add a second menu archive, so the trigger is a modified or replaced Misc BSA.
- Vanilla content does not trigger it (`vanilla_corpus`/`fo3_corpus` pass).

## Related
PAR-D1-2026-09-21-05 (sibling MenuXml parser finding); `include_cycles_terminate` (covers cycles only, not fan-out); `AUDIT_SAFETY_2026-09-21` states "menuxml … recursion is bounded" — true for tiles/ops, not for include chains

## Suggested Fix
- Pass `depth + 1` (or a separate include depth) into the spliced `parse_element_content`.
- Keep a per-document budget on total include splices and total tiles, with a warning when truncating.
- Normalise include keys by the resolved archive key, not the authored spelling.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-04)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix