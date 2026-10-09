# #5399: PAR-D1-2026-10-08-01: #5314's byte budget refuses a fragment but still caches it, so distinct refused includes retain 64 KiB each without bound

**Labels**: medium,import-pipeline,ui,memory,safety,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5399

**Source**: `docs/audits/AUDIT_PARSERS_2026-10-08.md` — `PAR-D1-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM
- **Dimension**: Size Discipline
- **Location**:
  - `crates/menuxml/src/parse.rs:831`: `includes.cache.insert(key.clone(), Some(bytes.clone()))` runs before the budget check.
  - `crates/menuxml/src/parse.rs:844-850`: the byte-budget refusal, which consumes neither `budget` nor `bytes_spent`.
  - `crates/menuxml/src/parse.rs:771-781`: the pre-fetch checks, which only stop once `bytes_spent >= MAX_INCLUDE_BYTES`.
- **Status**: NEW. It is a residual of #5314, whose commit message claims to close "the retained-bytes vector #5007 opened". That holds only for fragments over 64 KiB.
- **Trigger Input**:
  - A menu document whose includes first splice just under 256 KiB, for example three 64 KiB fragments plus one of 65,535 B, giving 262,143 B spent.
  - It then carries K further `<include src="gN.xml"/>` elements naming distinct prefabs, each between 2 B and 64 KiB.
  - Each prefab can be a separate entry in a mod `Misc.bsa`. A 64 KiB repetitive body compresses to roughly 100 B.
- **Description**:
  - Because `bytes_spent` stays one byte under the cap, the early return never fires.
  - Every new distinct path is fetched (a main-thread archive extract and inflate), inserted into the cache as `Some(bytes)`, and only then refused.
  - The refusal does not decrement the 256-splice budget. The number of such fetches is therefore bounded only by the number of include elements in the root document, and every refused fragment stays resident until `parse_document` returns.
- **Evidence** (`menuprobe`, release build, real `parse_document`, where the source answers each `menus\prefabs\` probe with a 64 KiB comment-only fragment):
  ```
  k=0      root  117 B  fetches     4  fetched     262,143 B  parse 0.17 ms  VmHWM 2,660 kB -> 3,064 kB
  k=1000   root   25 KB fetches 1,004  fetched  65,798,143 B  parse  31 ms   VmHWM 2,660 kB -> 66,436 kB
  k=10000  root  259 KB fetches 10,004 fetched 655,622,143 B  parse 240 ms   VmHWM 3,000 kB -> 644,568 kB
  ```
  Growth is exactly 64 KiB of resident memory per distinct refused include.
- **Impact**:
  - Main-thread memory spike and stall at `--hud` load: about 640 MiB per 10,000 distinct entries. With roughly 100 B compressed per entry, a mod archive of about 1.5 MB is enough.
  - The parse output itself stays bounded by #5314.
  - It is less amplifying than the original #5314 vector (about 450:1 against archive bytes, compared with millions to one), and the memory is released at the end of the parse. That is why this is rated MEDIUM, not HIGH.
  - Vanilla is unaffected: the heaviest vanilla document splices 38,378 B.
- **Related**: #5314, #5007, #4650, #5329. The #5329 fix will touch the same resolve block.
- **Suggested Fix**: do either of the following, and pin the probe's k=1000 case with a counting `MenuFileSource`:
  - Run the `bytes_spent + len > MAX_INCLUDE_BYTES` check before caching, and cache a refused fragment as `None` or as a length-only marker.
  - Or charge every fetch against a document-wide **fetched**-bytes budget, for example 2× `MAX_INCLUDE_BYTES`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the #5329 resolve block in `parse.rs` (same code))
- [ ] **TESTS**: A regression test pins this specific fix
