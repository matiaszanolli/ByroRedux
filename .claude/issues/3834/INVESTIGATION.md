# #3834 — investigation

## The suggested fix had a correctness hole

The issue proposed tracking "the touched cluster-index extent (running min/max)"
and uploading only that range. Applied literally to `fog_cluster_entries`, that
is **wrong**, and the failure is invisible to `cargo test`.

`build_fog_volume_clusters` resets `count = 0` for all 4096 entries every frame
and then sets it only for touched clusters. A cluster that was touched *last*
time this buffer was written and is untouched now must still have its zero
reach the GPU. Uploading only this frame's touched extent leaves the previous
non-zero `count` live in that buffer, and `sampleLocalMedium` reads
`fogClusterIndices[cluster.offset + i]` for `i < cluster.count` — so the shader
walks index slots belonging to a volume that no longer exists.

This is sharpened by the buffers being per-frame-in-flight: each is rewritten
only every N frames, so the stale data is N frames old, and the artifact would
be an intermittent flicker of fog where a volume used to be. Nothing in CI
touches a GPU, so no test would have caught it.

## What was implemented instead

A per-buffer high-water mark, `fog_cluster_dirty_hi[frame]`: the `entries`
prefix length that the last write to *that* buffer may have left non-zero.

```
write_hi = max(this_frame_hi, dirty_hi[frame])
upload entries[..write_hi]                       // zeros overwrite stale counts
upload indices[..write_hi * MAX_FOG_VOLUMES_PER_CLUSTER]
dirty_hi[frame] = this_frame_hi                  // everything above is now zero
```

Correctness rests on three facts, each pinned by a test or a code invariant:

1. **The CPU array is zero above `this_frame_hi`.** The reset loop zeroes all
   4096 counts before clustering, so uploading the prefix writes true zeros
   over anything stale within it.
   Pinned by `build_fog_volume_clusters_reports_a_hi_that_bounds_every_touched_cluster`.
2. **Nothing above `write_hi` is non-zero on the GPU.** Inductive: every prior
   write to this buffer covered `[0, its write_hi)` and recorded its own
   `this_frame_hi`; taking the max means this write cleans anything the
   previous one could have left. The base case is the seed —
   `fog_cluster_dirty_hi` starts at `FOG_VOLUME_CLUSTER_COUNT`, forcing the
   first write to each buffer to be complete, because `create_host_visible`
   does **not** zero the allocation.
3. **The index prefix follows the cluster prefix.** `offset` is
   `cluster_index * MAX_FOG_VOLUMES_PER_CLUSTER` (`fog_cluster_entries_with_offsets`,
   seeded once per #3133), so a contiguous cluster prefix maps to a contiguous
   index prefix. Asserted per-entry in the same test.

The empty-fog branch still skips the cluster/index writes entirely, unchanged,
and deliberately does **not** touch `dirty_hi` — the GPU still holds the old
prefix, so the mark must survive. #2242's `fogVolumeCount == 0u` early-out is
what makes that skip safe, and that argument is untouched here.

## Why `write_mapped_prefix` exists at all

Two of the three writes needed no new API: `entries` and `indices` are slices,
so `write_mapped(device, &slice[..n])` is already a prefix write, and
`write_mapped`'s non-coherent flush is already bounded to what was written
(#301). Only `GpuFogVolumeUpload` needed the new method, because its prefix
ends *inside* a `#[repr(C)]` struct (16-byte header + the populated leading
elements of a trailing fixed array), which no subslice can express.

The `unsafe` byte cast was factored into `byte_view` so the existing #3761
safety argument is stated once and shared, rather than copied into a second
entry point.

## What is NOT verified here

The byte-count reduction and the prefix invariants are pinned by CPU-side unit
tests. The **on-GPU** result — that fog still renders identically — was not
verified: this session had no GPU run. Given that this is the class of change
whose failure mode is invisible to `cargo test`, a visual check on a cell with
fire or smoke (and ideally `BYRO_VALIDATION=1`) is worth doing before relying
on it. `--game fo4 --cell DmndDugoutInn01` exercises authored fog.

## Measured effect

Per frame, in a cell with one small fog volume touching ~8 clusters:

| Buffer | before | after |
|---|---|---|
| `fog_volume_buffers` | 12,304 B | 112 B (`16 + 96`) |
| `fog_cluster_buffers` | 32,768 B | `8 × write_hi` B |
| `fog_cluster_index_buffers` | 131,072 B | `32 × write_hi` B |

Empty-fog frames drop from 12,304 B to 16 B, and that is every frame in most
interiors.
