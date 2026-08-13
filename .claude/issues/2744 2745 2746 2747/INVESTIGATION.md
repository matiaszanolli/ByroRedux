# Investigation notes

## #2744 — establishing which CameraUBO fields are absolute vs. relative

Before touching `cluster_cull.comp`, needed to confirm `cameraPos.xyz` (the UBO field
the fix subtracts `renderOrigin` from) is genuinely ABSOLUTE, not already relative —
getting this backwards would silently double-subtract instead of fixing anything.
Traced it through `crates/renderer/src/vulkan/context/draw.rs`: `GpuCamera.position`
is built from `effective_cam_pos`, itself derived from the RAW `camera_pos` field
of `FrameInputs` (the ECS-side absolute camera transform) via `dof_effective_view_proj`
— no origin rebase applied anywhere in that path. A comment right above that call site
(`draw.rs`, near `render_origin` construction) explicitly confirms the convention split:
passes that reconstruct world from an inverse VP either "add the origin back where
absolute space is required (**cluster_cull**, caustic_splat, volumetrics_inject) or
stay fully relative with a relative camera position (ssao, composite)". `cluster_cull`
is named in the first (absolute) group, confirming `camPos` there is absolute and
`camRel = camPos - renderOrigin.xyz` is the correct, lossless (both share the same
high bits at the snap point) fix.

**Sibling check** (per the issue's completeness list): looked at `caustic_splat.comp`
(the other shader `draw.rs`'s comment names in the "absolute" group) for the same
premature-difference shape. It has a structurally similar unproject-then-lift helper,
but the one place it differences two absolute positions afterward (`LtoG = G - Lp`,
surface-to-light for attenuation) is a general lighting-distance computation with no
guaranteed-small magnitude — unlike cluster_cull's near-plane corner (fixed ~0.1-unit
offset from the camera), a light can be arbitrarily far from the surface it's lighting,
so this isn't clearly the same bug shape. Not fixed here — flagging as a possible
follow-up worth its own audit pass rather than assumed broken and speculatively
"fixed" without evidence, consistent with this project's no-speculative-Vulkan-fixes
convention. `volumetrics_inject.comp` doesn't reference `renderOrigin` at all — the
"add origin back" comment in `draw.rs` describing it is either stale or refers to a
different mechanism; not investigated further as out of scope for #2744.

## #2745 — confirming the write-mask fix doesn't reopen the "denoiser smears" bug

The `preserve_opaque_gbuffer` axis exists specifically to stop glass's own
normal/motion/mesh-id/raw_indirect/albedo from overwriting the opaque surface behind
it (denoiser smears + "caustics through walls"). Before restoring mesh_id's write,
needed to confirm doing so wouldn't reintroduce that exact problem for mesh_id
specifically. Traced the consumer: `gbuffer.rs::MESH_ID_FORMAT`'s doc and
`svgf_temporal.comp` (`if (currID == 0u || (currID & 0x80000000u) != 0u) { ... no
history ... }`) confirm bit 31 (the alpha-blend flag `triangle.frag`'s `outMeshID`
already sets correctly for every blended fragment) is exactly the signal SVGF's
temporal-history gate uses to SKIP treating a pixel as a stable opaque surface
identity. So letting glass write its own bit-31-tagged mesh_id is safe by
construction — every consumer already discriminates on that bit before trusting the
value as a "same surface as last frame" identity, unlike normal/motion/raw_indirect
/albedo, which have no equivalent discriminator and would genuinely leak glass state
onto the opaque surface's slot if made writable.
