title:	REN-LIFE-NEW-01: shutdown leak — 8 outstanding Arc<Mutex<Allocator>> at try_unwrap (deterministic)
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, high, memory, renderer, safety, vulkan
comments:	0
assignees:	
projects:	
milestone:	
number:	927
--
**Severity**: HIGH (silent in release, panic in debug; 8 Arc clones leak every session → entire Vulkan instance + device + surface intentionally leaked to avoid UAF per #665 LIFE-L1's safety guard)

**Symptom**: On clean window-close shutdown of a Skyrim Markarth load, both `cargo run` (debug) and `cargo run --release` log:

\`\`\`
ERROR byroredux_renderer::vulkan::context]
  GPU allocator has 8 outstanding references — leaking allocator +
  device + surface + instance to avoid use-after-free on driver-side
  vkFreeMemory of late natural-Drop allocations. Process must
  terminate to reclaim.
\`\`\`

Debug additionally panics at \`crates/renderer/src/vulkan/context/mod.rs:1970\` (\`debug_assert!(false, ...)\`).

**Reproducibility**: deterministic. Exact count "8" reproduces across release and debug. Pre-exists b536299 (#905 fix) — unrelated to my recent resize-chain change.

**Repro recipe**:
\`\`\`bash
SKDATA="..."
cargo run --release -- \\
  --esm "\$SKDATA/Skyrim.esm" \\
  --wrld MarkarthWorld --grid 0,0 --radius 2 \\
  --bsa "\$SKDATA/Skyrim - Meshes0.bsa" \\
  --bsa "\$SKDATA/Skyrim - Meshes1.bsa" \\
  --textures-bsa "\$SKDATA/Skyrim - Textures0.bsa" \\
  ... (Textures1..8.bsa) \\
  > /tmp/markarth.log 2>&1
\`\`\`
Close the window. Grep \`/tmp/markarth.log\` for "outstanding references".

**Investigation so far** (this session):

- \`SceneBuffers::destroy\` (\`scene_buffer.rs:1422\`): destroys all 6 \`Vec<GpuBuffer>\` + 2 standalone (ray_budget, terrain_tile). **Complete.**
- \`TextureRegistry::destroy\` (\`texture_registry.rs:1220\`): drains \`pending_destroy\` queue, destroys all \`textures\` entries, takes \`staging_pool\` (\`#732 LIFE-N1\`). **Complete.**
- \`AccelerationManager::destroy\` (\`acceleration.rs:2741\`): drains \`pending_destroy_blas\`, destroys all \`blas_entries\` and per-frame \`tlas\` slots. **Complete.**
- \`BloomPipeline::destroy\` (\`bloom.rs:676\`): drains \`frames\` (per-mip down + up images, descriptor sets, param buffers). **Complete.**
- \`VolumetricsPipeline::destroy\` (\`volumetrics.rs:922\`): drains \`inject_volumes\` + \`integrated_volumes\` + \`param_buffers\` + \`integration_param_buffer\`. **Complete.**
- \`TaaPipeline::destroy\` (\`taa.rs:754\`): destroys \`param_buffers\`, history slots, pipelines, layouts, samplers. **Complete.**
- \`BloomPipeline\`, \`SsaoPipeline\`, \`VolumetricsPipeline\`, \`TaaPipeline\`, \`CausticPipeline\`, \`SvgfPipeline\`, \`CompositePipeline\`, \`GBuffer\`: none store \`SharedAllocator\` as a struct field — only takes it by reference at construction time. So none of these themselves leak Arc clones.

**Suspect: subsystems that clone the Arc explicitly**:

- \`StagingPool::new(device.clone(), allocator.clone())\` at \`texture_registry.rs:361\` — \`#732\` notes this previously leaked; the current take()-then-destroy() flow should release it. **Worth re-verifying with strong_count instrumentation.**
- \`mod.rs:1633\` \`flush_pending_destroys\` clones the Arc into a local; bound to function scope and dropped at return. Should be fine.

**Likely path forward**:

1. Add temporary \`log::info!("Arc strong_count after <subsystem>::destroy(): {}", Arc::strong_count(&alloc));\` after each destroy() call in \`VulkanContext::Drop\` (mod.rs:1837-1893).
2. Run Markarth, identify which destroy() leaves the count > 1.
3. Fix the destroy() to actually drop its Arc clone(s).

**Why HIGH severity**:

- Per \`feedback_vram_baseline.md\` budget is under ~4 GB. This leak intentionally leaks the whole VkInstance + VkDevice on shutdown so the OS reclaims via process termination — but if the engine is ever embedded (e.g. an editor that creates and destroys engine instances repeatedly), VRAM never returns.
- Validation-layer development workflow is broken in debug mode (every shutdown panics).

**Related (open)**:
- #856 (streaming worker JoinHandle held but never joined)
- #855 (TCP listener thread + per-client threads detached)
- Open audit findings REN-D7-NEW-03/05/06 from \`docs/audits/AUDIT_RENDERER_2026-05-09.md\` — none directly cover this, but they're in the same teardown-cleanup area.

**Not from #905**: my b536299 fix to composite/bloom/volumetric resize chain doesn't add Arc clones (the destroy+new pattern is net-zero per Arc count — same shape as the existing SSAO recreate). The leak exists regardless of the resize path firing. Verified by grepping the log: no \`Resized\` event triggered in the run that produced this output.

🤖 Filed during /fix-issue 905 verification — discovered this in the post-shutdown panic on the debug-build live-resize attempt.
