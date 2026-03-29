# Unified Severity Definitions

## CRITICAL
Happening now or will happen under normal use. Causes:
- Vulkan validation errors / GPU crashes / segfaults
- Memory leaks that grow per frame (GPU or CPU)
- Data corruption (wrong render output, ECS state corruption)
- Undefined behavior in unsafe blocks
No workaround exists. Must fix before any release.

## HIGH
Will fail under realistic conditions. Causes:
- Deadlocks under specific query patterns
- Resource leaks on swapchain recreate / window resize
- Incorrect synchronization (semaphore/fence misuse)
- Missing cleanup in Drop implementations
Fix before next milestone.

## MEDIUM
Incorrect behavior but workarounds exist. Causes:
- Inefficient GPU memory usage (wrong memory type, unnecessary copies)
- Missing error handling (unwrap on fallible operations)
- Suboptimal pipeline state (unnecessary state changes per frame)
- API design issues that will require breaking changes later
Fix within 2 milestones.

## LOW
Code quality / maintainability. Causes:
- Dead code, unused imports
- Missing documentation on public APIs
- Inconsistent naming conventions
- Test coverage gaps (but code works correctly)
Fix opportunistically.

## Classification Rules

- Severity is about **impact**, not likelihood
- Vulkan spec violations are HIGH minimum (even if driver tolerates them)
- Unsafe blocks without clear safety comments are MEDIUM minimum
- Memory/resource leaks are HIGH minimum (they compound per frame)
- If findings chain together to create a worse outcome, upgrade severity
