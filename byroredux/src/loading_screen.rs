//! Transition-owned loading presentation. The first backend draws the
//! installed legacy game's LSCR artwork and tip, not a bundled imitation.
//! Full legacy XML/NIF animation and Creation-era model menus remain TODO.

use crate::cell_loader::{LoadedCellIndex, PendingCellTransition};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{EsmIndex, LoadScreenRecord};
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::VulkanContext;

#[derive(Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    AwaitingPresentation,
    Presented,
    Loading,
    DestinationReady,
}

struct Artwork {
    key: String,
    texture: u32,
    tip: String,
}

#[derive(Default)]
pub(crate) struct LoadingScreen {
    phase: Phase,
    pending: Option<PendingCellTransition>,
    // One retained original image, reused on repeated transitions. This
    // avoids allocating a new non-reusable bindless slot at every door.
    artwork: Option<Artwork>,
    restore_capture: bool,
}

/// Until the contextual selector is connected, use only a valid,
/// unconditional legacy screen. Never display a location-specific tip at
/// an unrelated destination or treat malformed restrictions as absent.
fn unconditional_artwork(index: &EsmIndex) -> Option<&LoadScreenRecord> {
    if !matches!(index.game, GameKind::Oblivion | GameKind::Fallout3NV) {
        return None;
    }
    index
        .load_screens
        .values()
        .filter(|screen| {
            screen.malformed_fields.is_empty()
                && screen.locations.is_empty()
                && screen.conditions.is_empty()
                && !screen.icon.is_empty()
                && !screen.description.starts_with("<lstring ")
        })
        .min_by_key(|screen| screen.form_id)
}

impl LoadingScreen {
    pub(crate) fn active(&self) -> bool {
        self.phase != Phase::Idle
    }

    pub(crate) fn waiting_for_presentation(&self) -> bool {
        self.phase == Phase::AwaitingPresentation
    }

    pub(crate) fn texture(&self) -> Option<u32> {
        self.active()
            .then(|| self.artwork.as_ref().map(|a| a.texture))
            .flatten()
    }

    pub(crate) fn tip(&self) -> Option<&str> {
        self.active()
            .then(|| self.artwork.as_ref().map(|a| a.tip.as_str()))
            .flatten()
    }

    pub(crate) fn begin(
        &mut self,
        world: &byroredux_core::ecs::World,
        ctx: &mut VulkanContext,
        pending: PendingCellTransition,
    ) -> Result<(), PendingCellTransition> {
        // Own an Arc, not an ECS read guard, across archive I/O and GPU work.
        // In particular do not nest InputState access under the index lock.
        let Some(index) = world
            .try_resource::<LoadedCellIndex>()
            .map(|loaded| std::sync::Arc::clone(&loaded.0))
        else {
            return Err(pending);
        };
        let Some(screen) = unconditional_artwork(&index) else {
            return Err(pending);
        };
        let args = crate::cli_args::effective_args();
        // CLI archive/load-order identity is included because identical
        // Bethesda paths may name different images in different installs.
        let key = format!("{:?}:{:?}:{}", index.game, args, screen.icon);
        if self.artwork.as_ref().is_none_or(|art| art.key != key) {
            let provider = crate::asset_provider::build_texture_provider(&args);
            let Some(bytes) = provider.extract(&screen.icon) else {
                log::warn!("loading.screen: missing original artwork {}", screen.icon);
                return Err(pending);
            };
            let Some(allocator) = ctx.allocator.as_ref() else {
                return Err(pending);
            };
            let upload = GpuUploadCtx {
                device: &ctx.device,
                allocator,
                queue: &ctx.graphics_queue,
                command_pool: ctx.transfer_pool,
            };
            let texture = match ctx.texture_registry.load_dds_with_clamp(
                upload,
                &format!("loading-screen@{key}"),
                &bytes,
                0,
            ) {
                Ok(texture) => texture,
                Err(error) => {
                    log::warn!("loading.screen: artwork upload failed: {error:#}");
                    return Err(pending);
                }
            };
            if let Some(old) = self.artwork.take() {
                ctx.texture_registry.drop_texture(&ctx.device, old.texture);
            }
            self.artwork = Some(Artwork {
                key,
                texture,
                tip: screen.description.clone(),
            });
        } else if let Some(art) = self.artwork.as_mut() {
            art.tip.clone_from(&screen.description);
        }
        log::info!(
            "loading.screen: begin LSCR={:08X} art={}",
            screen.form_id,
            screen.icon
        );
        self.restore_capture = world
            .try_resource::<crate::components::InputState>()
            .is_some_and(|input| input.mouse_captured);
        self.pending = Some(pending);
        self.phase = Phase::AwaitingPresentation;
        Ok(())
    }

    pub(crate) fn take_presented_transition(&mut self) -> Option<PendingCellTransition> {
        if self.phase != Phase::Presented {
            return None;
        }
        self.phase = Phase::Loading;
        self.pending.take()
    }

    pub(crate) fn destination_ready(&mut self) {
        if self.active() {
            self.phase = Phase::DestinationReady;
        }
    }

    pub(crate) fn loading_started(&mut self) {
        if self.active() {
            self.phase = Phase::Loading;
        }
    }

    pub(crate) fn focus_lost(&mut self) {
        self.restore_capture = false;
    }

    pub(crate) fn take_restore_capture(&mut self) -> bool {
        !self.active() && std::mem::take(&mut self.restore_capture)
    }

    pub(crate) fn cancel(&mut self) {
        self.pending = None;
        self.phase = Phase::Idle;
    }

    /// Call only after a real submitted frame (not an out-of-date/zero-size
    /// early return). Keep the cover through one coherent destination frame.
    pub(crate) fn frame_presented(&mut self, geometry_ready: bool) {
        match self.phase {
            Phase::AwaitingPresentation => {
                self.phase = Phase::Presented;
                log::info!("loading.screen: presented before scene teardown");
            }
            Phase::DestinationReady if geometry_ready => {
                self.cancel();
                log::info!("loading.screen: dismissed after destination frame");
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_requires_presentation_and_a_ready_destination_frame() {
        let mut screen = LoadingScreen {
            phase: Phase::AwaitingPresentation,
            ..Default::default()
        };
        assert!(screen.take_presented_transition().is_none());
        assert!(screen.waiting_for_presentation());
        screen.frame_presented(false);
        assert_eq!(screen.phase, Phase::Presented);
        screen.take_presented_transition();
        assert_eq!(screen.phase, Phase::Loading);
        screen.frame_presented(true);
        assert!(screen.active());
        screen.destination_ready();
        screen.frame_presented(false);
        assert!(screen.active());
        screen.frame_presented(true);
        assert!(!screen.active());
    }

    #[test]
    fn superseding_a_ready_destination_keeps_the_cover_until_new_completion() {
        let mut screen = LoadingScreen {
            phase: Phase::DestinationReady,
            ..Default::default()
        };
        screen.loading_started();
        screen.frame_presented(true);
        assert!(screen.active());
        screen.destination_ready();
        screen.frame_presented(true);
        assert!(!screen.active());
    }

    #[test]
    fn cancellation_restores_capture_once_but_focus_loss_never_recaptures() {
        let mut screen = LoadingScreen {
            phase: Phase::Loading,
            restore_capture: true,
            ..Default::default()
        };
        assert!(!screen.take_restore_capture());
        screen.cancel();
        assert!(screen.take_restore_capture());
        assert!(!screen.take_restore_capture());
        screen.phase = Phase::Loading;
        screen.restore_capture = true;
        screen.focus_lost();
        screen.cancel();
        assert!(!screen.take_restore_capture());
    }

    #[test]
    fn selector_never_widens_restricted_or_malformed_screens() {
        let mut index = EsmIndex::default();
        index.load_screens.insert(
            1,
            LoadScreenRecord {
                form_id: 1,
                icon: "test.dds".into(),
                ..Default::default()
            },
        );
        assert!(unconditional_artwork(&index).is_some());
        index
            .load_screens
            .get_mut(&1)
            .unwrap()
            .malformed_fields
            .push(*b"LNAM");
        assert!(unconditional_artwork(&index).is_none());
        let record = index.load_screens.get_mut(&1).unwrap();
        record.malformed_fields.clear();
        record.conditions.push(Default::default());
        assert!(unconditional_artwork(&index).is_none());
        index.load_screens.get_mut(&1).unwrap().conditions.clear();
        index.load_screens.get_mut(&1).unwrap().locations.push(
            byroredux_plugin::esm::records::LoadScreenLocation {
                direct: 123,
                world: 0,
                grid_x: 0,
                grid_y: 0,
            },
        );
        assert!(unconditional_artwork(&index).is_none());
        index.load_screens.get_mut(&1).unwrap().locations.clear();
        index.game = GameKind::Skyrim;
        assert!(unconditional_artwork(&index).is_none());
    }
}
