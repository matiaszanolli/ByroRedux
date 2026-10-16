# #5897: PAR-D5-2026-10-15-01: 856 vanilla Fallout 4 clips are `hkaLosslessCompressedAnimation`, which the decoder does not read

- **Labels**: medium,bug,animation,game:fo4
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: 856 vanilla Fallout 4 clips are `hkaLosslessCompressedAnimation`, which the decoder does not read. They fail as "class missing", and the only report is a debug log.

- **Severity**: MEDIUM. The 841 first-person clips have no consumer yet. The 15 third-person clips are reachable now through FO4 actor graphs' lazy clip resolution: 12 `animations\paired`, 2 `furniture\coupleholding`, 1 `deathclaw`, plus 1 `pipboy`. Under the skill rule "vanilla content failing = HIGH", this becomes HIGH once first-person or paired playback is consumed.
- **Dimension**: Decode-Consumer Wiring
- **Location**:
  - `crates/hkx/src/animation.rs:400` (`pack.object("hkaSplineCompressedAnimation")?` is the only clip class read)
  - Consumers: `actor_graph.rs:272-274` (`log::debug!`, then `job.lazy.fail`), `object_graph.rs:99-103`, `archetype_idle.rs:152-154`
- **Status**: NEW. There is no issue for lossless or non-spline FO4 clips.
- **Description**:
  - The FO4 census: 13,990 clips under `\animations\`.
    - 13,133 decode.
    - 744 are `hkaLosslessCompressedAnimation` + `hkaDefaultAnimatedReferenceFrame`.
    - 112 are `hkaLosslessCompressedAnimation` alone.
    - 1 is a bare `hkaAnimationContainer`.
  - All 857 fail with `HKX object class 'hkaSplineCompressedAnimation' is missing`, which reads as file corruption rather than "unsupported compression".
  - In `actor_graph.rs` the failure logs at debug level. The follow-up log at `:299` then says the clip was "not found", which misdiagnoses it.
- **Evidence**: `/tmp/audit/parsers/probe-hkx/census.txt` and `missing.txt`
  - first-person 841, e.g. `meshes\actors\character\_1stperson\animations\laserpistol\wpnafterjigglefireautoadd.hkx`
  - third-person e.g. under `meshes\actors\character\animations\paired\`
- **Trigger Input**: a vanilla FO4 clip whose animation object is `hkaLosslessCompressedAnimation`.
- **Suggested Fix**:
  - Return `UnsupportedLayout("hkaLosslessCompressedAnimation")` when the container's animation is a known non-spline class, and warn in the consumers.
  - Decode the lossless format (Havok 2014 quantised key arrays), or record it as a parked format in the census (see D4-01).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
