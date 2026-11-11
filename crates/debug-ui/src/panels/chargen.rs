//! The native character-creation page: the screens a script opens while the player builds
//! their character (New Vegas's opening at Doc Mitchell's: name, sex and race, the
//! Vit-o-matic's SPECIAL, tag skills, traits). The Game UI Integration PRD's surface for these
//! menus is the game's own MenuXml document (S2); until the MenuXml track serves non-HUD menus,
//! this page stands in for them (GUI-PAGE-002, Q4: "a native page otherwise").
//!
//! The page keeps an editable draft per screen, seeded from the snapshot when a screen opens
//! (its `serial` changes), and hands the accepted draft back as a [`ChargenChoice`].

use egui::{Color32, RichText};

use super::{ChargenChoice, ChargenScreenView, ChargenSnapshot, PanelOutputs};

/// The page's working copy of the open screen's choices.
#[derive(Debug, Clone, Default)]
pub struct ChargenDraft {
    /// The screen the draft was seeded for.
    serial: Option<u64>,
    name: String,
    female: bool,
    race: u32,
    special: Vec<i32>,
    tags: Vec<u32>,
    traits: Vec<u32>,
}

impl ChargenDraft {
    fn seed(&mut self, snapshot: &ChargenSnapshot) {
        if self.serial == Some(snapshot.serial) {
            return;
        }
        *self = ChargenDraft {
            serial: Some(snapshot.serial),
            name: snapshot.name.clone(),
            female: snapshot.female,
            race: snapshot.race,
            special: snapshot.attributes.iter().map(|(_, value)| *value).collect(),
            tags: snapshot.tagged.clone(),
            traits: snapshot.chosen_traits.clone(),
        };
    }

    /// The choice the draft stands for, or `None` while it is not a valid answer to the screen.
    fn choice(&self, snapshot: &ChargenSnapshot) -> Option<ChargenChoice> {
        match &snapshot.screen {
            ChargenScreenView::Name => {
                let name = self.name.trim();
                (!name.is_empty()).then(|| ChargenChoice::Name(name.to_string()))
            }
            ChargenScreenView::RaceSex => Some(ChargenChoice::RaceSex { female: self.female, race: self.race }),
            ChargenScreenView::Special { points, .. } => (self.special.iter().sum::<i32>() == *points).then(|| {
                ChargenChoice::Special(snapshot.attributes.iter().zip(&self.special).map(|((attribute, _), value)| (attribute.id, *value)).collect())
            }),
            ChargenScreenView::TagSkills { count } => (self.tags.len() == *count).then(|| ChargenChoice::TagSkills(self.tags.clone())),
            ChargenScreenView::Traits { max } => (self.traits.len() <= *max).then(|| ChargenChoice::Traits(self.traits.clone())),
        }
    }
}

/// Draw the open character-creation screen. With no screen (the snapshot raced the close),
/// the page offers only a way back to the game.
pub(super) fn draw_chargen_page(ui: &mut egui::Ui, snapshot: Option<&ChargenSnapshot>, draft: &mut ChargenDraft, outputs: &mut PanelOutputs) {
    let Some(snapshot) = snapshot else {
        if ui.button("Close").clicked() {
            outputs.resume_game = true;
        }
        return;
    };
    draft.seed(snapshot);
    ui.heading(snapshot.title.clone());
    ui.add_space(10.0);
    egui::ScrollArea::vertical().id_salt("chargen_screen").max_height(430.0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        match &snapshot.screen {
            ChargenScreenView::Name => {
                let edit = ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(360.0).char_limit(32));
                if !edit.has_focus() && draft.name.is_empty() {
                    edit.request_focus();
                }
            }
            ChargenScreenView::RaceSex => draw_race_sex(ui, snapshot, draft),
            ChargenScreenView::Special { points, min, max } => draw_special(ui, snapshot, draft, *points, *min, *max),
            ChargenScreenView::TagSkills { count } => draw_picks(ui, &snapshot.skills.iter().map(|(skill, value)| (skill, Some(*value))).collect::<Vec<_>>(), &mut draft.tags, *count),
            ChargenScreenView::Traits { max } => draw_picks(ui, &snapshot.traits.iter().map(|chosen| (chosen, None)).collect::<Vec<_>>(), &mut draft.traits, *max),
        }
    });
    ui.add_space(14.0);
    let choice = draft.choice(snapshot);
    let done = ui.add_enabled(choice.is_some(), egui::Button::new(RichText::new("Done").size(18.0)).min_size(egui::vec2(160.0, 38.0)));
    if done.clicked() {
        outputs.chargen_choice = choice;
    }
}

fn draw_race_sex(ui: &mut egui::Ui, snapshot: &ChargenSnapshot, draft: &mut ChargenDraft) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut draft.female, false, RichText::new("Male").size(17.0));
        ui.selectable_value(&mut draft.female, true, RichText::new("Female").size(17.0));
    });
    ui.add_space(8.0);
    for race in &snapshot.races {
        ui.selectable_value(&mut draft.race, race.id, RichText::new(race.name.clone()).size(16.0));
    }
}

fn draw_special(ui: &mut egui::Ui, snapshot: &ChargenSnapshot, draft: &mut ChargenDraft, points: i32, min: i32, max: i32) {
    let left = points - draft.special.iter().sum::<i32>();
    ui.label(RichText::new(format!("Points left: {left}")).size(16.0).color(Color32::from_rgb(225, 222, 210)));
    ui.add_space(6.0);
    egui::Grid::new("chargen_special").num_columns(4).spacing([16.0, 6.0]).show(ui, |ui| {
        for ((attribute, _), value) in snapshot.attributes.iter().zip(draft.special.iter_mut()) {
            ui.label(RichText::new(attribute.name.clone()).size(16.0));
            if ui.add_enabled(*value > min, egui::Button::new("-")).clicked() {
                *value -= 1;
            }
            ui.label(RichText::new(value.to_string()).size(16.0));
            if ui.add_enabled(*value < max && left > 0, egui::Button::new("+")).clicked() {
                *value += 1;
            }
            ui.end_row();
        }
    });
}

/// A pick list: up to `max` entries, each a toggle; a value shown beside each when it has one.
fn draw_picks(ui: &mut egui::Ui, options: &[(&super::ChargenOption, Option<i32>)], picked: &mut Vec<u32>, max: usize) {
    ui.label(RichText::new(format!("{} / {max}", picked.len())).size(16.0).color(Color32::from_rgb(225, 222, 210)));
    ui.add_space(6.0);
    for (option, value) in options {
        let mut on = picked.contains(&option.id);
        let label = match value {
            Some(value) => format!("{}  {value}", option.name),
            None => option.name.clone(),
        };
        let response = ui.add_enabled(on || picked.len() < max, egui::Checkbox::new(&mut on, RichText::new(label).size(16.0)));
        if !option.description.is_empty() {
            response.clone().on_hover_text(option.description.clone());
        }
        if response.changed() {
            if on {
                picked.push(option.id);
            } else {
                picked.retain(|id| *id != option.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::ChargenOption;

    fn snapshot(screen: ChargenScreenView) -> ChargenSnapshot {
        let option = |id: u32| ChargenOption { id, name: format!("option {id}"), description: String::new() };
        ChargenSnapshot {
            screen,
            serial: 1,
            title: String::new(),
            name: "Courier".into(),
            female: false,
            race: 0x19,
            races: vec![option(0x19), option(0x38)],
            attributes: (5..=11).map(|code| (option(code), 5)).collect(),
            skills: (32..=45).map(|code| (option(code), 15)).collect(),
            tagged: Vec::new(),
            traits: vec![option(0x10), option(0x11), option(0x12)],
            chosen_traits: Vec::new(),
        }
    }

    /// SPECIAL is accepted only with every point spent: New Vegas's Vit-o-matic hands out 40
    /// over seven attributes that start at 5 (35), so five are left to place.
    #[test]
    fn special_is_accepted_only_with_every_point_spent() {
        let snapshot = snapshot(ChargenScreenView::Special { points: 40, min: 1, max: 10 });
        let mut draft = ChargenDraft::default();
        draft.seed(&snapshot);
        assert_eq!(draft.choice(&snapshot), None, "five points left");
        draft.special[0] += 5;
        assert_eq!(draft.choice(&snapshot), Some(ChargenChoice::Special((5..=11).map(|code| (code, if code == 5 { 10 } else { 5 })).collect())));
    }

    /// Tag skills need exactly the count asked for; traits any number up to the cap; a name
    /// must not be blank.
    #[test]
    fn picks_and_names_validate_against_the_screen() {
        let tags = snapshot(ChargenScreenView::TagSkills { count: 3 });
        let mut draft = ChargenDraft::default();
        draft.seed(&tags);
        draft.tags = vec![32, 33];
        assert_eq!(draft.choice(&tags), None);
        draft.tags.push(41);
        assert_eq!(draft.choice(&tags), Some(ChargenChoice::TagSkills(vec![32, 33, 41])));

        let traits = snapshot(ChargenScreenView::Traits { max: 2 });
        let mut draft = ChargenDraft::default();
        draft.seed(&traits);
        assert_eq!(draft.choice(&traits), Some(ChargenChoice::Traits(Vec::new())), "no trait is a valid answer");
        draft.traits = vec![0x10, 0x11, 0x12];
        assert_eq!(draft.choice(&traits), None);

        let name = snapshot(ChargenScreenView::Name);
        let mut draft = ChargenDraft::default();
        draft.seed(&name);
        draft.name = "   ".into();
        assert_eq!(draft.choice(&name), None);
        draft.name = " Six ".into();
        assert_eq!(draft.choice(&name), Some(ChargenChoice::Name("Six".into())));
    }

    /// A new screen reseeds the draft; the same screen keeps the player's edits.
    #[test]
    fn the_draft_is_seeded_once_per_screen() {
        let mut snapshot = snapshot(ChargenScreenView::RaceSex);
        let mut draft = ChargenDraft::default();
        draft.seed(&snapshot);
        draft.female = true;
        draft.seed(&snapshot);
        assert!(draft.female);
        snapshot.serial = 2;
        draft.seed(&snapshot);
        assert!(!draft.female);
    }
}
