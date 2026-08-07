# Issues 1827, 1843, 1981, 2067

## #1827 — FO4-D4-02: Starfield BSGeometry leaves per-vertex bone indices/weights empty (informational, out of FO4 scope)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:169-173`. LOW, informational — not an FO4 defect.

`extract_skin_bs_geometry` resolves bind matrices but intentionally leaves per-vertex bone indices/weights empty (packed BSGeometry vertex bone channel not decoded). Starfield skinned meshes render in bind pose. Issue's own suggested fix: "Track as Starfield skinning work (separate milestone)" — not actionable as a small fix.

## #1843 — NIF-D1-01: Pre-4.1 NIF bool fields read as 1 byte where the wire format is 32-bit
**Location**: `crates/nif/src/blocks/base.rs:279`, `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:326,349,379,421-422`, `crates/nif/src/blocks/texture.rs:913`; family likely extends further.

nif.xml: bool is 32-bit up to and including v4.0.0.2, 8-bit from v4.1.0.1 on. `NifStream::read_bool` implements this correctly; the listed sites bypass it with fixed 1-byte `read_byte_bool`/`read_u8`. On a real v4.0.0.2 (Morrowind-era) file each such bool under-reads 3 bytes — unrecoverable cascade (no `block_sizes` table in this band). Latent today (band out of the Oblivion→Starfield compat matrix) but the parser claims to support it.

**Fix**: replace fixed-width reads with `stream.read_bool()` at the listed sites; sweep for other pre-4.1-reachable `read_byte_bool` sites; fix the synthetic Morrowind fixture to write 4-byte bools at v4.0.0.2; correct the `read_byte_bool` doc comment.

## #1981 — FNV-D7-02: Skinned-mesh WorldBound does not track a ragdoll that leaves its origin (cull/RT-bound pop)
**Location**: `byroredux/src/ragdoll.rs:324-377` (`ragdoll_writeback_system`), `byroredux/src/systems/bounds.rs` (`make_world_bound_propagation_system`).

Ragdoll writeback moves bone `GlobalTransform`s but never the skinned-mesh root entity; `WorldBound` derives from `LocalBound × GlobalTransform` on the mesh entity, which stays bind-pose-anchored since ragdoll bones carry no `LocalBound`. A ragdoll that slides/falls away from spawn can be frustum-culled or get a stale TLAS bound while still on-screen. Does not violate the Late-write/`LocalBound` invariant.

**Fix**: when a `RagdollActive` actor's simulated bodies leave the mesh's bind-pose radius, expand/recenter the mesh `WorldBound` from the live bone globals.

## #2067 — TD2-108: NiSingleInterpController prologue reimplemented inline at 4 sites instead of calling NiSingleInterpController::parse
**Location**: canonical `controller/mod.rs:253-267` (`NiSingleInterpController::parse`); duplicated at `controller/shader.rs:56-63,180-186,212-219`, `controller/mod.rs:594-600`.

4 controllers (`NiLightColorController`, `NiMaterialColorController`, `NiTextureTransformController`, `NiFloatExtraDataController`) re-implement the identical 8-line `parse_interp_controller_base` + conditional `interpolator_ref` prologue instead of calling the shared `NiSingleInterpController::parse` wrapper that 2 sibling controllers already use correctly.

**Fix**: call `NiSingleInterpController::parse(stream)?` and destructure at each of the 4 sites. Purely mechanical.
