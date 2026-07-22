#pragma once

// FidelityFX SDK v1.1.4 contains two case-inconsistent includes that only work
// on case-insensitive filesystems. Keep the upstream source unchanged and
// forward those spellings to the canonical public headers on Linux.
#include <FidelityFX/host/ffx_types.h>
