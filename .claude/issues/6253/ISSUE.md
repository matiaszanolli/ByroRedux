# #6253: LC-2026-10-21-D2-01: The spawn clip of a mesh with both standalone controllers and a sequence drops the sequence's cycle type and all of its non-transform channels

**Labels**: medium,bug,legacy-compat,animation,nif,game:oblivion,game:fnv

**Source**: docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 2 — Animation model (timing envelope + channel kinds) · **Location**: `crates/nif/src/anim/entry.rs:832-856` (`import_embedded_animations_with_sequences`), and its premise in the doc comment at `:820-823`. Callers: `byroredux/src/cell_loader/references/import.rs:101-102`, `byroredux/src/streaming/pre_parse.rs:326`.

- **Severity**: MEDIUM. Authored animation data is parsed, then dropped at the merge, with visible results: flags and gates stop animating after one cycle, and emitters the sequence hides keep emitting.
- **Dimension**: 2 — Animation model (timing envelope + channel kinds)
- **Location**: `crates/nif/src/anim/entry.rs:832-856` (`import_embedded_animations_with_sequences`), and its premise in the doc comment at `:820-823`. Callers: `byroredux/src/cell_loader/references/import.rs:101-102`, `byroredux/src/streaming/pre_parse.rs:326`.
- **Status**: NEW. The code dates from `fb74b409b` (#3602, 2026-08-30). No open or closed issue covers it.
- **Description**: When a NIF yields a standalone-controller clip, the merge keeps that clip and copies in only the first sequence's `channels` (transform), `duration` (max of the two), `accum_root_name` and `text_keys`. Three things are lost:
  - The sequence's `float_channels`, `color_channels`, `bool_channels` and `texture_flip_channels`.
  - The sequence's `cycle_type`, `frequency` and `phase`. The standalone clip's envelope wins.
  
  The doc's premise, "sequences carry the `channels` (transform) map, standalone controllers carry the float/color/bool/texture-flip vectors", is false on vanilla content. The one test, `embedded_import_with_sequences_picks_up_a_nicontrollersequence`, only covers the branch where the standalone clip is `None`.
- **Evidence** (probe `envelope`, object meshes only, characters and creatures excluded):

  | Archive | NIFs with both | Envelope differs | Sequence non-transform channels dropped |
  |---|---:|---:|---:|
  | Oblivion | 11 | 4 | 11 |
  | FNV | 7 | 5 | 1 |
  | FO3 | 1 | 0 | 0 |

  - FNV `clutter\flags\nv_ncr_flag.nif`, `nv_nevadaflag.nif` and `nv_foa_flag.nif`: standalone `Clamp` against `Idle` `Loop` (9-10 transform channels). The flag wave plays once and stops.
  - Oblivion `oblivionarchgate01.nif`: standalone `Clamp` (dur 1) against `SpecialIdle` `Loop` (dur 5, 107 transform channels). The merged clip is `Clamp`, dur 5. All 64 `SpecialIdle` visibility channels are dropped. At t=0, `SpecialIdle` hides `dustEmitter` and `MagmaEmitter`, but both are visible by NIF flags, so they render and emit when the authored idle hides them.
  - `obliviongate_simple.nif` and `obliviongate_forming.nif` show the same pattern.
  - In the other direction, FNV `dlc04brainpod01.nif` and `braincontroldevice01.nif` (`Talking`, `Clamp`) are forced to `Loop`.
- **Impact**:
  - Placed vanilla flags across FNV exteriors stop waving after their first cycle.
  - Every Oblivion gate's idle (rotating rubble and tendril transforms) runs once and freezes, and shows effects the idle sequence keeps hidden.
  - The same merged clip is what the streaming pre-parse registers, so exterior streaming is affected too.
- **Related**: #3602 (the fix that introduced the merge), #3345 (phase), D2-02, #4128.
- **Suggested Fix**:
  - Stop merging two animations into one clip. Spawn two players (or two `AnimationStack` layers) on the placement root: the standalone-controller clip with its own envelope, and the first sequence's clip intact.
  - At minimum, extend the merge to the four non-transform vectors and keep the *sequence's* envelope for its channels.
  - Add a test with both sources present that asserts the sequence's `bool_channels` and `cycle_type` survive.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
