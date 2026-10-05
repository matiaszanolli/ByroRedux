# #5314: PAR-D1-2026-10-05-01: The MenuXml include budget counts splices, not bytes, so one small compressed fragment expands into gigabytes of tiles

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5314
- **Labels**: high,import-pipeline,bug,ui,memory,safety,game:oblivion,game:fo3,game:fnv
- **Source**: `docs/audits/AUDIT_PARSERS_2026-10-05.md` (PAR-D1-2026-10-05-01)

_From `docs/audits/AUDIT_PARSERS_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: HIGH
- **Dimension**: Size Discipline
- **Location**: `crates/menuxml/src/parse.rs:588` (`MAX_INCLUDE_SPLICES`), `crates/menuxml/src/parse.rs:597-637` (`IncludeState`), `crates/menuxml/src/parse.rs:710-800` (`splice_include`). Production fetch: `byroredux/src/hud.rs:209-211` (`HudAssets::menu_xml` → `extract_or_warn`). Parse sites: `crates/menuxml/src/menu.rs:147,170,316`.
- **Status**: NEW. This is a residual of #4650: the exponential blow-up is fixed, and the remaining growth is 256× linear.
- **Trigger Input**: a menu document carrying up to 256 `<include src="g.xml"/>` elements, plus a prefab `menus\prefabs\g.xml` whose body is N copies of `<rect/>`. Either file can come from a mod `Oblivion - Misc.bsa` / `Fallout - Misc.bsa`, where the last-listed archive wins. The repetitive body compresses about 700:1.
- **Description**:
  - #4650 bounded includes by nesting (`depth + 1`) and by a global count of 256 splices. #5007 then made the fetch O(1) per path.
  - Nothing bounds the *bytes* spliced or the *tiles* produced. Each splice re-parses the whole fragment into fresh `TileSeed`s, so output is up to 256 × the fragment.
  - The 48-level tile cap limits depth only. There is no total tile cap.
  - #5007's cache also keeps every distinct fetched fragment's bytes alive for the whole `parse_document` call. Before #5007, each fragment was dropped after its splice.
- **Evidence** (probe `menuprobe`, release build; the root carries 300 includes and the budget stops at 256):
  ```
  fragment   4 KiB (  585 tiles): fetches 1, document tiles   149,761, parse  20 ms, VmHWM +28 MiB
  fragment  16 KiB ( 2340 tiles): fetches 1, document tiles   599,041, parse  82 ms, VmHWM +114 MiB
  fragment  64 KiB ( 9362 tiles): fetches 1, document tiles 2,396,673, parse 328 ms, VmHWM +458 MiB
  fragment 256 KiB (37449 tiles): fetches 1, document tiles 9,586,945, parse 2.08 s, VmHWM +1,834 MiB
  zlib -9: a 1 MiB fragment = 1,559 B; a 4 MiB fragment = 6,130 B
  ```
  Growth is exactly linear, at about 7.2 MiB resident per KiB of fragment. A 1 MiB fragment therefore costs about 7.3 GiB and a 4 MiB fragment about 29 GiB.
- **Impact**:
  - Out-of-memory, or a multi-second stall, on the main thread at `--hud` load. The HUD then evaluates and lays out every tile each frame.
  - The trigger is a few KB inside a mod archive.
  - The skill's severity rule applies: an OOM reachable from an archive or mod file is HIGH.
  - Vanilla corpora are unaffected: 89 documents and 1,868 tiles.
- **Related**: #4650, #5007, PAR-D1-2026-09-21-04, PAR-D1-2026-09-29-02
- **Suggested Fix**:
  - Add a per-document byte budget for spliced text, for example a small multiple of the largest vanilla prefab, measured from the three corpora.
  - Or add a total tile cap in `parse_element_content`, or both.
  - Stop splicing and warn once (`warn_once("budget", …)`) when either budget is exhausted.
  - Pin it with the probe's 64 KiB × 256 case.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
