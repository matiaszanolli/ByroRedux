//! Persistent, constant-rate restorative effects. No entity or pool handles.
use super::ActorValues;
use crate::ecs::{Component, SparseSetStorage};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct TimedRestoration {
    pub source_form_id: u32,
    pub actor_value: u32,
    pub per_second: f32,
    /// Simulation seconds remaining, not game-clock hours.
    pub remaining: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct TimedRestorations {
    pub effects: Vec<TimedRestoration>,
}

impl Component for TimedRestorations {
    type Storage = SparseSetStorage<Self>;
}

impl TimedRestorations {
    /// Tick only the unexpired part of a frame. Restoring never creates an AV
    /// or banks overheal for later damage. Invalid saved/runtime rows expire.
    pub fn advance(&mut self, values: &mut ActorValues, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.effects.retain_mut(|effect| {
            if !effect.remaining.is_finite()
                || effect.remaining <= 0.0
                || !effect.per_second.is_finite()
                || effect.per_second <= 0.0
            {
                return false;
            }
            let elapsed = effect.remaining.min(f64::from(dt));
            if let Some(value) = values.get(effect.actor_value) {
                if value.current().is_finite() && value.damage.is_finite() {
                    // Compute in f64 and cap before converting: even a corrupt
                    // large rate/duration must not overflow the damage layer.
                    let amount = (f64::from(effect.per_second) * elapsed)
                        .min(f64::from(value.damage.max(0.0)))
                        as f32;
                    values.restore(effect.actor_value, amount);
                }
            }
            effect.remaining -= elapsed;
            effect.remaining > 0.0
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duration_clamps_last_frame_and_does_not_bank_overheal() {
        let mut values = ActorValues::from_pairs([(7, 100.0)]);
        let mut active = TimedRestorations {
            effects: vec![TimedRestoration {
                source_form_id: 1,
                actor_value: 7,
                per_second: 10.0,
                remaining: 2.5,
            }],
        };
        active.advance(&mut values, 1.0); // full health: first second is lost
        values.apply_damage(7, 60.0);
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            active.advance(&mut values, dt);
            assert_eq!(active.effects[0].remaining, 1.5);
        }
        active.advance(&mut values, 1.0);
        assert_eq!(values.current(7), 50.0);
        active.advance(&mut values, 10.0);
        assert_eq!(values.current(7), 55.0);
        assert!(active.effects.is_empty());
        active.advance(&mut values, 10.0);
        assert_eq!(values.current(7), 55.0);
    }
}
