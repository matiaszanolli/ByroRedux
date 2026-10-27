# #6315 — why the suggested fix does not hold

The issue proposes folding the hashes `MaterialTable` already computes. They are not the same hash:

- `MaterialTable.index` is keyed on `hash_gpu_material_fields(&material)` — every lane, including the
  per-frame-animated ones (UV offset/scale, the four material colours, alpha, shader colour/float,
  emissive).
- `fold_svgf_scene_inputs` hashes `svgf_material_hash(material)`, which zeroes exactly those lanes first
  (#6247) so one animated material (waterfall, conveyor, magic shader) does not re-key SVGF history
  every frame.

Folding the table's stored hashes would reintroduce the #6247 regression (whole image pinned at the
recovery alpha floor). The upload gate's `hash_material_slice` has the same problem and is also computed
later, per frame-in-flight, inside `upload_materials`.

A stable hash cached at intern-miss time costs the same once-per-unique-material hash the fold does now,
so it removes nothing. The measured cost (14 ns/material, 0.06–0.11 ms at 4–8 k unique) is the floor of
hashing 432 B once per unique material; the only real saving would be skipping the fold when no
material lane changed, which needs a cross-frame material-table diff — more machinery than the cost
justifies. Left open as not worth fixing as filed.
