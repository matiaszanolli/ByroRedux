## VKC-006: No CI job runs a frame under Vulkan validation layers

**Severity**: INFO
**Domain**: vulkan / CI
**Source audit**: AUDIT_SAFETY_2026-06-01.md

Already fixed in commit `22a1af5e` — `vulkan-validation` job added to
`.github/workflows/ci.yml` using Mesa lavapipe. Issue just was not closed.
