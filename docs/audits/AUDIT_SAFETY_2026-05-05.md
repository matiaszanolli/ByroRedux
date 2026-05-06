# Safety Audit — 2026-05-05

**Auditor**: Claude Opus 4.7 (1M context)
**Baseline**: `docs/audits/AUDIT_SAFETY_2026-05-03.md` (2 days ago) — left SAFE-22 (NEW) plus carried-open SAFE-09 / 11 / 12 / 14 / 15 / 19 / 20 / 21.
**Scope**: Delta audit since 05-03. Major change: M44 audio crate (`crates/audio/src/lib.rs`, ~1829 lines, kira backend, 6 phases) plus ~50 commits across the rest of the tree. The 05-03 baseline left 9 items open; this audit re-checks each plus the new audio surface.
**Open-issue baseline**: cached at `/tmp/audit/issues.json` (per audit framework — not re-fetched).

---

## Executive Summary

| Severity | NEW | Carried Open | Fixed since 2026-05-03 |
|----------|-----|--------------|------------------------|
| CRITICAL | 0   | 0            | — |
| HIGH     | 0   | 0            | — |
| MEDIUM   | 0   | 2 (SAFE-09 / 11 / 20) | 1 (SAFE-22 fixed via #797) |
| LOW      | 2 (SAFE-23, SAFE-24) | 5 (SAFE-12 / 14 / 15 / 19 / 21) | 0 |

**Headline**: No CRITICAL findings. The M44 audio crate landed clean: zero unsafe blocks across all 1829 lines, kira handle drop ordering is correct (`active_sounds` → `reverb_send` → `listener` → `manager` matches the field declaration order), all production paths return `Result` instead of panicking on bad bytes, and headless / no-device degradation is graceful (every `play_*` short-circuits cleanly when `manager.is_none()`). The two NEW LOW findings are: (a) non-looping `ActiveSound` continues playing past entity despawn — audible but bounded; (b) `SoundCache` is defined but has zero call sites in the binary today — the documented "no eviction" policy is dormant, not live.

SAFE-22 is **closed** in this delta — `MaterialTable::intern` now caps at `MAX_MATERIALS` and returns id `0` on overflow with `Once`-gated warn (verified at `crates/renderer/src/vulkan/material.rs:375-394`). The 05-03 follow-up sibling check on the warn message at `scene_buffer.rs:975` is now truthful.

**CRITICAL list**: none.

---

## Findings by Dimension

### Dimension 1 — Unsafe Rust Blocks

#### Audio crate (M44)

`grep -nE '^[[:space:]]*unsafe ' crates/audio/src/lib.rs` returns **zero hits**. The crate composes safe Rust over kira's safe API, with no FFI or pointer-extension idioms. No new `// SAFETY` comments were added because none were needed.

No findings here.

---

### Dimension 2 — Vulkan Spec Compliance

No new compute pipelines or render passes added since 05-03 (the audio crate is CPU-only). VulkanContext drop chain at `context/mod.rs:1528-1670` re-verified — unchanged from 05-03; M44 added zero Vulkan resources.

No new findings.

---

### Dimension 3 — Memory Safety

#### NEW — LOW

##### SAFE-23 — Non-looping `ActiveSound` survives entity despawn until natural termination

- **Severity**: LOW
- **Dimension**: Memory Safety / Audio Lifecycle
- **Locations**:
  - `crates/audio/src/lib.rs:817-870` — `prune_stopped_sounds` only stops looping sounds when their entity loses `AudioEmitter`. Non-looping handles are dropped on `PlaybackState::Stopped` only.
  - `byroredux/src/cell_loader.rs:258-262` — `unload_cell` calls `world.despawn(eid)` on every cell-owned entity. No coordination with `AudioWorld::active_sounds`.
- **Status**: NEW
- **Description**: `unload_cell` despawns every cell entity (which removes ALL component rows including `AudioEmitter`). For **looping** active sounds, `prune_stopped_sounds` notices the missing `AudioEmitter` and issues `handle.stop(Tween::default())`. For **non-looping** active sounds (footsteps, weapon fire, dialogue), the prune path takes no action — kira keeps decoding and mixing the sound until natural termination (typically 50 ms - 3 s for short SFX).

  This is **not a memory leak**: `ActiveSound` is bounded by playback duration and self-prunes on `Stopped`. EntityIds are never recycled in this engine (`world.rs:113-116` per #372 / #36), so `s.entity == Some(stale_eid)` is safely a no-op when `prune_stopped_sounds` reaches it (despawn already removed all components, the `emitter_q.remove(entity)` call is idempotent).

  The audible effect: a footstep that lands at the last frame of cell A's tick finishes playing through cell B's first ~150 ms. Spatial position is stale (the `_track` is anchored at the despawned entity's last `GlobalTransform`, not updated thereafter). For exterior cell streaming this would be unnoticeable; for fast-travel interior-to-interior transitions this could surface as a faint cross-cell SFX bleed.
- **Evidence**:
  ```rust
  // crates/audio/src/lib.rs:826-840 — only looping path checks emitter presence
  for (idx, s) in audio_world.active_sounds.iter().enumerate() {
      if !s.looping {
          continue;                          // ← non-looping sounds skip the despawn check
      }
      let Some(entity) = s.entity else { continue; };
      let still_has_emitter = emitter_q
          .as_ref().map(|q| q.get(entity).is_some()).unwrap_or(false);
      if !still_has_emitter {
          to_stop_indices.push(idx);
      }
  }
  ```
- **Impact**: Audible only on cross-cell transitions during active SFX playback. No memory growth, no leak, no GPU coupling. Footstep system runs in `Stage::Update` and dispatch happens in `Stage::Late` of the same frame, so the exposure window per cell unload is at most one playback duration (~3 s ceiling).
- **Suggested Fix** (sketch — do NOT ship without observing the actual audible regression first; per `feedback_speculative_vulkan_fixes.md`): widen the looping-only branch in `prune_stopped_sounds` to ALSO stop non-looping sounds whose `entity` no longer has `AudioEmitter`. Three-line change:
  ```rust
  for (idx, s) in audio_world.active_sounds.iter().enumerate() {
      let Some(entity) = s.entity else { continue; };  // queue-driven entries unaffected
      let still_has_emitter = emitter_q.as_ref()
          .map(|q| q.get(entity).is_some()).unwrap_or(false);
      if !still_has_emitter {
          to_stop_indices.push(idx);
      }
  }
  ```
  This makes cell unload truncate every cell-owned active sound regardless of looping. Side note: the symmetric question of "queue-driven (`entity == None`) one-shots that out-live their cell" doesn't apply — no entity, no despawn coupling, they always run to natural termination, which is the intended semantics of `play_oneshot`.
- **Related**:
  - The looping path at `lib.rs:826-846` was added in M44 Phase 4. Extending it to non-looping is a single-condition relaxation.
  - No GitHub issue.

#### NEW — LOW

##### SAFE-24 — `SoundCache` Resource defined but has zero call sites in the binary

- **Severity**: LOW
- **Dimension**: Memory Safety (dormant API) / Documentation accuracy
- **Locations**:
  - `crates/audio/src/lib.rs:951-1041` — `SoundCache` definition with documented "Eviction strategy: **none today**" policy.
  - Zero hits for `SoundCache` outside `crates/audio/src/lib.rs` (`grep -rn 'SoundCache' /mnt/data/src/gamebyro-redux/byroredux/ /mnt/data/src/gamebyro-redux/crates/ | grep -v 'crates/audio/src/lib.rs'`).
- **Status**: NEW
- **Description**: The audit-audio dispatch flagged `SoundCache` unbounded growth as a watchpoint. Investigation: `SoundCache` is dead code in the current tree. Nothing in the engine binary calls `SoundCache::new()`, `insert()`, `get()`, or `get_or_load()`. The current footstep dispatch path at `byroredux/src/asset_provider.rs:251-252` writes directly into `FootstepConfig.default_sound: Option<Arc<Sound>>` — bypassing the cache entirely. The decoded Arc is held by exactly one `Resource` (FootstepConfig) for the engine lifetime; multi-sound SFX paths haven't landed yet (FOOT records, REGN ambient) so there's no second consumer to drive cache lookups.

  The "no eviction → unbounded growth" concern from the prompt does not produce a live leak today because the cache never grows past `len() == 0`. The risk surfaces the moment a future commit wires a real consumer — the dormant API ships with explicit "no eviction" semantics and no telemetry beyond `len()`.
- **Evidence**: Zero non-test references; the `Default` impl, `is_empty`, `len`, `insert`, `get`, and `get_or_load` are exercised only by `#[cfg(test)] mod tests` (lines 1124-1185).
- **Impact**: No live impact today. Future wiring (planned Phase 3.5b: FOOT records → per-material sound lookup) will produce cache growth proportional to the unique-sound count. Vanilla FNV `Fallout - Sound.bsa` is 6,465 entries (~620 MB on disk); 100% load with the typical 5–10× decompression ratio gives 3–6 GB of decoded PCM in worst case. That's a real memory footprint, but the docstring's "few hundred MB" estimate is conservative for FNV (sit-on-it for now), and a workable answer for the future is an LRU bolted on at the cache layer.
- **Suggested Fix**: leave the API as-is until a real consumer lands. When wiring FOOT-driven dispatch, add (a) a soft cap (e.g. 256 distinct sounds, ~256 MB ceiling for short SFX) and (b) LRU eviction. For now, the only deliverable from this audit is **upgrading the docstring** to flag dormancy and pin the future cap discussion:
  ```rust
  /// **Status (2026-05-05)**: defined but unused — no consumer in the
  /// engine binary today. The "no eviction" docstring is aspirational.
  /// When the first real consumer lands (FOOT records, REGN ambient),
  /// add an LRU cap before exterior streaming wires up.
  ```
- **Related**:
  - `feedback_no_guessing.md` — the "few hundred MB" estimate was speculative when written; verify actual decoded size when FOOT lands.
  - The `pending_oneshots: Vec` cap at 256 entries (`lib.rs:327`) is the right precedent for what `SoundCache` should look like once it has a consumer.

#### Verified — no new memory-safety gaps

- `pending_oneshots` queue capped at 256 entries with FIFO drop-oldest (`lib.rs:327-335`). The `Vec::remove(0)` at `lib.rs:334` is O(n) for n=256 — measurable but not a safety bug, performance only.
- `active_sounds` retained via `retain()` semantics (`lib.rs:849-862`); each entry self-prunes on `Stopped`. No unbounded growth observed.
- `music: Option<StreamingSoundHandle>` is single-slot; replacing through `play_music` correctly tweens-out the old before assigning the new (`lib.rs:372-389`).
- `reverb_send: Option<SendTrackHandle>` is created once at `AudioWorld::new()` and never reassigned (`lib.rs:251-269`).

---

### Dimension 4 — Thread Safety

The audio crate is single-threaded by design — `AudioWorld` is a `Resource` (single-writer ECS) and `audio_system` is a serial system. kira's internal threading (the audio renderer thread) is encapsulated behind the `AudioManager` opaque handle, with no shared state crossing the boundary except through kira's own thread-safe API.

No `unsafe impl Send/Sync` blocks in the audio crate. `Resource for AudioWorld` and `Resource for SoundCache` are marker-trait implementations only.

The 05-03 carryover items (SAFE-09 N>2 multi-query ordering, SAFE-11 pipeline cache from CWD) are unchanged. Re-verified open. Still benign under the sequential scheduler.

No new findings.

---

### Dimension 5 — FFI Safety (cxx bridge)

The audio crate uses no FFI. The cxx-bridge surface is unchanged from 05-03 — one struct, two functions, no raw pointer exchange. No findings.

---

### Dimension 6 — RT Pipeline Safety

No RT changes since 05-03. M44 is CPU-only. The audio prompt's call-out for RT pipeline risk is N/A here. Carryover items unchanged.

No new findings.

---

### Dimension 7 — New Compute Pipeline Safety (TAA / Caustic / Skin)

No new compute pipelines added since 05-03. The audio crate adds none. Carryover SAFE-20 (`// SAFETY` comment counts on caustic / taa / ssao / svgf / composite / gbuffer modules) re-verified at:

| Module | `// SAFETY` count | unsafe block count |
|---|---|---|
| caustic.rs | 0 | 19 |
| taa.rs | 0 | 17 |
| ssao.rs | 0 | 10 |
| svgf.rs | 0 | 18 |
| composite.rs | 0 | 25 |
| gbuffer.rs | 1 | 9 |

Counts are unchanged from 05-03. SAFE-20 / #579 remains MEDIUM, carried open.

---

### Dimension 8 — R1 Material Table Safety

#### CLOSED since 2026-05-03

| Issue | Closed by | Verification |
|---|---|---|
| `SAFE-22` (#797) | one-line cap landed | `material.rs:375-394` — `intern()` returns `0` past `MAX_MATERIALS` with `INTERN_OVERFLOW_WARNED.call_once`. The `scene_buffer.rs:975` warn-message-without-impl is now truthful. |

The 260 B size pin (`material.rs:432`) and per-field offset pin (`gpu_material_field_offsets_match_shader_contract` at `material.rs:459`) both still in place.

#### Carryover — unchanged

- `#807` (R1-N7 — `material_id == 0` overloaded as default-init / first-interned / over-cap fallback) remains open. Not raised to a finding here because the 05-03 audit flagged it indirectly via SAFE-22's "share material 0" semantics, and #807 is the issue tracking the fix. The over-cap path now reuses material 0 deterministically, which is the correct degradation per #797's design rationale (NaN-or-zero material is the safest GPU-OOB miss substitute) but does mean the `material_id == 0` slot is overloaded. Not in this audit's regression scope.

No new findings in this dimension.

---

### Dimension 9 — RT IOR-Refraction Safety

Re-verified at `triangle.frag`:

- Glass-passthrough loop (#789): `glassIORAllowed = (old + 2u <= GLASS_RAY_BUDGET)` at `triangle.frag:1397-1400`. Budget 8192. Loop guard at `:1482-1492`. Unchanged.
- Frisvad orthonormal basis (#820 / REN-D9-NEW-01): `buildOrthoBasis` at `triangle.frag:295-320`. Confirmed singularity-free Frisvad (sign-pivot at `dir.z = -1` only). Unchanged.
- IOR miss fallback for interiors (bb53fd5): cell-ambient path verified. Unchanged.
- `DBG_VIZ_GLASS_PASSTHRU = 0x80` debug bit catalog at `triangle.frag:628-686`. New audio-side `DBG_BYPASS_NORMAL_MAP = 0x10` bit (per CLAUDE.md feedback) doesn't collide.

No new findings.

---

### Dimension 10 — NPC / Animation Spawn Safety

No animation changes since 05-03. The audio crate touches neither B-spline nor `AnimationClipRegistry`. #772 (FLT_MAX sentinel) and #790 (case-insensitive interning) verified unchanged from 05-03.

No new findings.

---

## Cross-Cutting

None this audit. SAFE-23 is purely an audio-lifecycle concern; SAFE-24 is dead-code documentation. The kira drop-ordering verification cuts across Dimensions 2 (Vulkan-pattern teardown discipline) and 3 (memory safety), but the actual order in `AudioWorld` field declarations matches the documented intent — no cross-cutting bug surfaces.

---

## Verified Working — kira drop ordering

The `AudioWorld` struct field-declaration order at `crates/audio/src/lib.rs:185-217`:

```
1. active_sounds: Vec<ActiveSound>           // owns SpatialTrackHandles
2. pending_oneshots: Vec<PendingOneShot>     // owns Arc<StaticSoundData> only
3. music: Option<StreamingSoundHandle>       // streaming track on main bus
4. reverb_send: Option<SendTrackHandle>      // reverb send track
5. reverb_send_db: f32                       // POD
6. listener: Option<ListenerHandle>          // kira listener
7. manager: Option<AudioManager>             // kira audio manager
```

Drop order = declaration order = correct teardown chain:
- Spatial sub-tracks (in `active_sounds._track`) drop before listener (kira requires sub-tracks die before parent).
- Streaming music handle drops before main-bus owner (manager).
- Send track drops before manager.
- Listener drops before manager.
- Manager drops last.

The docstring at `lib.rs:181-184` correctly documents this contract. No drop-ordering bugs. The "Drop ordering bugs in kira typically manifest as assert-fail on shutdown" risk from the prompt does not apply here.

---

## Verified Working — symphonia decoder error paths

`load_sound_from_bytes` (`lib.rs:924-927`) returns `Result<StaticSoundData, FromFileError>`. Bad bytes from a BSA extract surface as `Err(FromFileError)` — no panic. The two production call sites (`asset_provider.rs:244` and the same in tests) handle the error via `match` and log+continue. Symphonia internal panics on truly malformed inputs are kira/symphonia upstream concerns, not this audit's scope; in practice the `2026-05-03` real-data integration test (`real_fnv_sounds_decode_through_kira`) covers WAV+OGG happy paths against vanilla FNV and the `get_or_load` test (`lib.rs:1152-1185`) covers the error path explicitly with synthesised junk bytes.

---

## Priority Action Items

1. **SAFE-22** — verified closed via #797 (MaterialTable cap). 05-03 follow-through is done.
2. **SAFE-23** (this audit, LOW) — three-line condition-relaxation in `prune_stopped_sounds` to truncate non-looping sounds on emitter despawn. Defer until cross-cell SFX bleed is observed in practice (per `feedback_speculative_vulkan_fixes.md`); ship with a real-data regression test.
3. **SAFE-24** (this audit, LOW) — docstring update on `SoundCache` flagging dormancy. No code change.
4. **SAFE-20 / #579** (carryover, MEDIUM) — `// SAFETY` comment sweep across caustic / taa / ssao / svgf / composite. Counts unchanged in 12 days; the priority is unchanged.
5. **SAFE-09 / 11 / 12 / 14 / 15 / 19 / 21** (carryover) — unchanged. No live bug, hardening only.

The audio surface introduces no CRITICAL or HIGH findings. M44 is a clean drop.

---

## Methodology Notes

- Issue dedup against the cached snapshot at `/tmp/audit/issues.json` (~200 entries; not re-fetched per audit framework).
- Each carryover from 2026-05-03 re-verified by direct grep against the current tree.
- Audio crate read in full (1829 lines, 3 chunks). Drop-ordering claim verified by reading `AudioWorld` field-declaration order against the docstring claim at `lib.rs:181-184`.
- `SoundCache` consumer search via `grep -rn 'SoundCache' crates/ byroredux/ | grep -v 'crates/audio/src/lib.rs'` returned zero hits — driven the LOW reclassification on SAFE-24.
- `Vec::remove(0)` cost concern at `lib.rs:334` evaluated and dropped: 256-element shift is performance, not safety. Skipped per `feedback_no_guessing.md` (no concrete trigger condition).
- Per `feedback_audit_findings.md` discipline: the prompt-supplied watchpoints (BSA → bad bytes → panic; `active_sounds` Vec growth across cells; SoundCache unbounded growth) were each disprovable as written — bytes path returns `Result`, `active_sounds` self-prunes on `Stopped`, `SoundCache` is dormant. The two LOW findings represent what survived disproof.
- No sub-agent dispatches. Per the 05-03 methodology note, sub-agents stall reliably on this size of audit.

---

*Generated by `/audit-safety` on 2026-05-05. To file findings as GitHub issues: `/audit-publish docs/audits/AUDIT_SAFETY_2026-05-05.md`.*
