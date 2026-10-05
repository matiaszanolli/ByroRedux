# #5329: PAR-D6-2026-10-05-01: #5007's pre-fetch cycle check rejects a distinct higher-priority include whenever a lower-priority spelling is on the stack

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5329
- **Labels**: low,import-pipeline,bug,ui,game:oblivion,game:fo3,game:fnv
- **Source**: `docs/audits/AUDIT_PARSERS_2026-10-05.md` (PAR-D6-2026-10-05-01)

_From `docs/audits/AUDIT_PARSERS_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/menuxml/src/parse.rs:729-749` (`splice_include` candidate keys and cycle check)
- **Status**: NEW (introduced by `2465740cc`, the #5007 fix)
- **Trigger Input**: a fragment resolved via the 2nd or 3rd candidate spelling (e.g. authored `menus\x.xml`) that includes `x.xml`, while a *different* file exists at the first-priority `menus\prefabs\x.xml`.
- **Description**:
  - #5007 tests **all three** candidate keys against the nesting stack before fetching: `keys.iter().any(|k| includes.seen.…contains(k))`.
  - Before #5007, the cycle key was the *resolved* candidate, meaning the first spelling that exists. If any lower-priority spelling of an include equals an ancestor, the include is now rejected as a cycle, even though resolution would have picked a different, higher-priority file.
- **Evidence** (probe `menuprobe --bin fp`):
  - Setup: `menus\x.xml` = `<rect name="outer"><include src="x.xml"/></rect>` and `menus\prefabs\x.xml` = `<rect name="prefab_x"/>`. The root includes `menus\x.xml`.
  - Result: the tiles are `["root", "outer"]`. `prefab_x` is dropped with the warning "include cycle on 'x.xml'".
  - Under the pre-#5007 logic, `menus\prefabs\x.xml` resolves first, is not on the stack, and is spliced.
- **Impact**: a mod menu or prefab silently loses content when it mixes `menus\`-relative and bare spellings. Vanilla includes resolve via `menus\prefabs\` and pass, but the corpus prints its 1,868-tile total without pinning it.
- **Related**: #5007, #4650, PAR-D1-2026-10-05-01
- **Suggested Fix**:
  - Resolve first through the cache. The cache already makes a repeat probe O(1), so moving the fetch back ahead of the check does not reintroduce the quadratic cost.
  - Then cycle-check only the resolved key.
  - Pin the probe case, and pin the vanilla tile total in `vanilla_corpus.rs`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
