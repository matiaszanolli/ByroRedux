# #956: Replace debug_assert! inside cmd recording with log::warn!
# draw.rs:1260-1267
# debug_assert!(gpu_instances.len() <= MAX_INSTANCES) panics inside active recording
# Fix: log::warn! if overflow, let upload_instances handle clamping
