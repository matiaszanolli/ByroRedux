# Investigation: Issue #61

## Root Cause
Every texture creates its own VkSampler with identical settings
(LINEAR/REPEAT). Two creation sites:
- from_rgba (line 250): LINEAR/REPEAT, no mipmaps
- from_bc_data (line 519): LINEAR/REPEAT, max_lod varies by mip count

## Fix
1. Add shared_sampler to TextureRegistry, created once in new()
2. Use max_lod = 16.0 (driver clamps to actual mip chain length)
3. Texture construction accepts sampler instead of creating one
4. Texture::destroy() no longer destroys the sampler
5. TextureRegistry destroys the shared sampler on cleanup

## Scope
2 files: texture.rs (accept sampler, stop destroying), texture_registry.rs
(create shared sampler, pass to texture construction, destroy on cleanup).
Also context.rs for the fallback texture construction in new().
