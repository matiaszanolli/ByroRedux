//! Debug-UI load queue — typed requests pushed by the debug-server
//! (which only has `&World` access) and drained by the binary's main
//! loop where `&mut World + &mut VulkanContext + Provider`s are all
//! held.
//!
//! Mirrors the deferred-execution shape of `PendingCellTransition` in
//! the binary's `cell_loader` module: the trigger site can only stage
//! a queued op, the consumer runs once per frame in the App's
//! between-frames step. The shape lives in core so the debug-server
//! crate (which can't depend on the binary) can construct values for
//! it.

use super::resource::Resource;

/// One queued load operation. The variants line up 1:1 with the
/// [`byroredux_debug_protocol::DebugRequest::LoadNif`] /
/// [`byroredux_debug_protocol::DebugRequest::LoadInteriorCell`] /
/// [`byroredux_debug_protocol::DebugRequest::LoadExteriorCell`]
/// wire-protocol variants — the server's evaluator translates one
/// into the other.
#[derive(Debug, Clone)]
pub enum PendingDebugLoad {
    /// Load a single NIF mesh.
    Nif {
        /// NIF path. Absolute filesystem (loose) or archive-relative
        /// (`meshes\foo.nif`) resolved through the active BSA / BA2
        /// set.
        path: String,
        /// Diagnostic label. Defaults to the basename of `path`
        /// when omitted by the request.
        label: Option<String>,
    },
    /// Load an interior cell by editor ID.
    InteriorCell {
        esm: String,
        cell: String,
        masters: Vec<String>,
        bsas: Vec<String>,
        textures_bsas: Vec<String>,
    },
    /// Load an exterior grid.
    ExteriorCell {
        esm: String,
        grid_x: i32,
        grid_y: i32,
        /// Streaming radius. Clamped to `1..=7` by the consumer to
        /// match the CLI `--radius` cap.
        radius: u8,
        worldspace: Option<String>,
        masters: Vec<String>,
        bsas: Vec<String>,
        textures_bsas: Vec<String>,
    },
}

/// Resource slot — a FIFO of queued load ops. Always present at
/// engine boot (inserted alongside `PendingCellTransitionSlot`), so
/// write sites with only `&World` access can push via `resource_mut`
/// without needing to structurally insert.
///
/// Multiple queued requests are allowed (operator clicks "load" three
/// times — drain in order). The consumer processes them sequentially
/// in `step_debug_loads` to avoid cross-load state corruption.
#[derive(Debug, Default)]
pub struct PendingDebugLoadSlot(pub Vec<PendingDebugLoad>);

impl Resource for PendingDebugLoadSlot {}

impl PendingDebugLoadSlot {
    pub fn push(&mut self, load: PendingDebugLoad) {
        self.0.push(load);
    }

    /// Drain every queued op. Always returns the queued ops in FIFO
    /// order; the slot is empty after the call. Mirrors the
    /// `take_pending_transition` contract used by the cell-transition
    /// orchestrator.
    pub fn drain(&mut self) -> Vec<PendingDebugLoad> {
        std::mem::take(&mut self.0)
    }
}

/// Requested runtime upscaler change, staged for the main loop.
///
/// Same deferred shape as [`PendingDebugLoadSlot`], and for the same reason:
/// the console and the settings panel only reach `&World`, while switching
/// the reconstruction path needs `&mut VulkanContext` plus the live window
/// size. The value is the CLI spelling (`taa`, `fsr3`, `fsr3 quality`, …) so
/// there is exactly one grammar for selecting an upscaler; core cannot name
/// the renderer's `UpscalerMode` type directly.
///
/// Last request wins — clicking through three presets in the settings panel
/// should perform one switch, not three.
#[derive(Debug, Default)]
pub struct PendingUpscalerSwitch(pub Option<String>);

impl Resource for PendingUpscalerSwitch {}

impl PendingUpscalerSwitch {
    pub fn request(&mut self, spec: impl Into<String>) {
        self.0 = Some(spec.into());
    }

    pub fn take(&mut self) -> Option<String> {
        self.0.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upscaler_switch_keeps_only_the_last_request() {
        let mut slot = PendingUpscalerSwitch::default();
        slot.request("fsr3 quality");
        slot.request("fsr3 performance");
        assert_eq!(slot.take().as_deref(), Some("fsr3 performance"));
        assert!(slot.take().is_none());
    }

    /// Two pushes drain in insertion order and leave an empty slot —
    /// the only contract `App::step_debug_loads` actually relies on.
    #[test]
    fn drain_returns_fifo_and_empties() {
        let mut slot = PendingDebugLoadSlot::default();
        slot.push(PendingDebugLoad::Nif {
            path: "a.nif".into(),
            label: None,
        });
        slot.push(PendingDebugLoad::Nif {
            path: "b.nif".into(),
            label: Some("second".into()),
        });

        let drained = slot.drain();
        assert_eq!(drained.len(), 2);
        match &drained[0] {
            PendingDebugLoad::Nif { path, .. } => assert_eq!(path, "a.nif"),
            _ => panic!("unexpected variant"),
        }
        match &drained[1] {
            PendingDebugLoad::Nif { path, label } => {
                assert_eq!(path, "b.nif");
                assert_eq!(label.as_deref(), Some("second"));
            }
            _ => panic!("unexpected variant"),
        }

        assert!(slot.0.is_empty(), "slot must be empty after drain");
        assert!(
            slot.drain().is_empty(),
            "second drain on empty slot must be a no-op"
        );
    }
}
