# #3834: PERF-D1-2026-09-05-01: `VolumetricsPipeline::dispatch` re-uploads the full 176 KB fog cluster/index/volume staging set to write-combined memory every frame, to convey a few hundred meaningful bytes

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-2026-09-05-01) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3834 --json state`.

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D1-2026-09-05-01), published from `/audit-suite volumetrics-deep`. Premise re-verified against HEAD at publish time.

> Note: `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: CPU Hot Paths
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:2211-2225` (writes), `:1196-1227` (buffer creation), `:456-545` (`build_fog_volume_clusters`)
- **Status**: NEW
- **Description**: Every frame in which the scene has at least one local fog
  volume, `dispatch` performs three whole-array `write_mapped` calls into
  `MemoryLocation::CpuToGpu` buffers (write-combined, per `buffer.rs:998-1004`'s
  own doc comment):

  | Buffer | Bytes written per frame | Bytes actually meaningful |
  |---|---|---|
  | `fog_volume_buffers[frame]` (`GpuFogVolumeUpload`) | `16 + 128 × 96` = **12,304** | `16 + volume_count × 96` |
  | `fog_cluster_buffers[frame]` (`[GpuFogClusterEntry; 4096]`) | `4096 × 8` = **32,768** | 8 × number of clusters a volume touched |
  | `fog_cluster_index_buffers[frame]` (`[u32; 32768]`) | `32768 × 4` = **131,072** | 4 × `sum(entry.count)` |
  | **total** | **176,144 B/frame** | typically **< 1 KB** |

  The index buffer is the pathological one. `build_fog_volume_clusters`
  populates only `indices[entry.offset + i]` for `i < entry.count` — the doc
  comment at `:472-482` correctly argues the rest never needs *resetting* —
  but the code then uploads the whole 128 KB array regardless. With
  `FOG_VOLUME_CLUSTER_DIM = 16` over a `2 × grid_far_meters = 256 m` grid, one
  cluster cell is 16 m on a side, so a typical metre-scale flame/smoke volume
  touches 1–8 clusters — a 4,000–32,000× write amplification on the index
  buffer alone.
- **Evidence**:
```rust
// volumetrics.rs:2216-2225
if !fog_volumes.is_empty() {
    self.fog_cluster_buffers[frame].write_mapped(
        device,
        std::slice::from_ref(self.fog_cluster_entries.as_ref()),   // 32,768 B
    )?;
    self.fog_cluster_index_buffers[frame].write_mapped(
        device,
        std::slice::from_ref(self.fog_cluster_indices.as_ref()),   // 131,072 B
    )?;
}
```
  `write_mapped` (`buffer.rs:1256-1281`) is an unconditional
  `mapped[..len].copy_from_slice(&bytes[..len])` — no dirty-range notion. The
  same file's own #301 comment ("The instance SSBO is 1.28 MB but a typical
  frame writes only a few KB — flushing the full range wastes bandwidth")
  already establishes partial-range discipline as a recognised requirement;
  the volumetrics call sites just don't supply a bounded range. The
  unconditional 12,304-byte `fog_volume_buffers` write at `:2212-2215` also
  runs on the *empty* branch, where only the 16-byte `count` header is read
  by the shader (`fogVolumeCount == 0u` early-out, `:2183-2191`).
- **Impact**: ~176 KB of uncached/write-combined host writes per frame in any
  cell with fire, smoke, steam or an authored fog box; ~12 KB per frame
  everywhere else. At ~1.5–4 GB/s WC store throughput on a discrete part,
  the fog-bearing case is ~45–120 µs of pure `memcpy` per frame (0.3–0.7% of
  a 16.6 ms budget) for data that is almost entirely zeros — not a
  frame-killer today, but exactly the class of avoidable CPU cost this
  dimension targets on a 16-core part, and it scales cubically with any
  future `FOG_VOLUME_CLUSTER_DIM` bump.
- **Related**: #3133 (the `offset`-seeding fix that removed the per-frame
  *recompute* but left the per-frame *upload*), #301 (partial flush ranges),
  #2242 (the empty-branch `fogVolumeCount` invariant that makes the skip
  safe).
- **Suggested Fix**: Have `build_fog_volume_clusters` track the touched
  cluster-index extent (running min/max) and pass that range to a new
  `GpuBuffer::write_mapped_range`, so only the touched slice of `indices`/
  `entries` crosses the bus; bound the `GpuFogVolumeUpload` write to
  `16 + volume_count * size_of::<GpuFogVolume>()` at the same time.
- **Confidence**: High — static read of the write call sites and the buffer
  memory-location doc; no engine run needed to confirm the shape of the
  waste, though the exact µs figure is estimated from typical WC bandwidth,
  not measured.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
