# #3563 — REN-2026-08-30-D3-01: the `DBG_*` u32 flag mask is fully exhausted — all 32 bits are allocated and no test guards uniqueness or headroom

**Labels**: `medium,renderer,shaders,test-gap,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3563 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/shader_constants_data.rs` (`DBG_BITS`, `DBG_RESERVED_20`, `DBG_RESERVED_200`), `crates/renderer/src/shader_constants.rs` (`dbg_bits_catalog_covers_every_dbg_constant`)
- **Status**: New
- **Description**: The `DBG_*` debug-visualisation bitfield carried in
  `GpuCamera.render_debug.x` has consumed every bit of its `u32`. There is no free
  bit left to allocate, and the one census guard that exists cannot detect the
  failure mode that exhaustion creates — a new `DBG_*` constant that *aliases* an
  already-assigned bit.
- **Evidence**: Machine-counted over the live source, not quoted:
  ```
  single-bit DBG_* constants: 32
  union mask:                 0xffffffff
  free bits remaining:        0
  reserved placeholders:      ['DBG_RESERVED_20', 'DBG_RESERVED_200']
  ```
  `DBG_BYPASS_POM = 0x1` … `DBG_VIZ_SELECTED_LIGHT = 0x80000000` (bit 31) covers
  bits 0-31 with no gaps. The `DBG_BITS` catalog holds **35** entries — the 32
  single bits plus 3 compound unions (`DBG_VIZ_MATERIAL_LOBES`, `DBG_VIZ_RT_LOD`,
  `DBG_VIZ_SHADOW_VISIBILITY`). Only two slots are recyclable: `DBG_RESERVED_20`
  (bit 5) and `DBG_RESERVED_200` (bit 9).

  The sole census guard, `dbg_bits_catalog_covers_every_dbg_constant`
  (`shader_constants.rs:86`), compares `DBG_BITS.len()` against a **text count** of
  `pub const DBG_…: u32 =` lines in the data file. It asserts nothing about the
  *values*. A 33rd bit — which on a full `u32` can only be written as a duplicate of
  an existing value — gets a catalog entry, passes this test, passes
  `generated_header_contains_all_defines`, passes `triangle_frag_dbg_bits_not_redeclared`,
  and ships as two debug views silently firing each other.

  The codebase already has the exact guard this needs, applied to the *other*
  flag field: `instance_flag_bits_unique_and_outside_packed_windows`
  (`crates/renderer/src/vulkan/scene_buffer/constants.rs:454`) asserts
  `a.count_ones() == 1` per flag and `a & b == 0` pairwise. `INSTANCE_FLAG_*` — which
  has bits 4, 5 and 9-15 still free — is defended; `DBG_*`, which has none, is not.
- **Impact**: Adding any new debug view is now impossible without either silently
  aliasing an existing bit (undetected by the whole test suite) or knowing to
  recycle one of the two `DBG_RESERVED_*` slots — a fact recorded nowhere. Debug-path
  only, so no shipping-frame corruption, but the next person to add a debug view is
  set up to produce a confusing, test-green miscompare during exactly the kind of
  investigation debug views exist to serve.
- **Suggested Fix**: (1) Add `dbg_bits_are_single_bit_and_pairwise_disjoint`,
  modelled on `instance_flag_bits_unique_and_outside_packed_windows`, walking
  `DBG_BITS` and skipping the three known compound unions by name; assert the union
  of the single bits equals `u32::MAX` *and* emit the count of free bits so the
  exhaustion is visible in test output. (2) Document the two `DBG_RESERVED_*` slots
  as the allocation pool in the `DBG_BITS` doc comment. (3) For real expansion,
  `GpuCamera.render_debug` is a `uvec4` whose `.w` lane is unused — shaders read only
  `.x` (mode), `.y` (`rtLodScale`, itself a float smuggled through a uint lane via
  `uintBitsToFloat`) and `.z` (`rtLodTelemetryEnabled`) in `triangle.frag:141/789/793`.
  A second flag word costs zero bytes.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D3-01

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
