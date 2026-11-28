//! The Markdown report. Its order is fixed, so an unchanged install regenerates it byte for byte.

use super::roles::{Role, base_set_description};
use super::scan::{BaseClip, Displacement, GameCensus, is_footfall};
use std::collections::BTreeMap;
use std::fmt::Write;

fn share(count: usize, of: usize) -> String {
    format!("{count} / {of}")
}

fn clips_where(census: &GameCensus, role: Option<Role>, test: impl Fn(&BaseClip) -> bool) -> usize {
    census
        .base
        .iter()
        .filter(|c| role.is_none_or(|r| c.role == r) && test(c))
        .count()
}

fn has_footfall(clip: &BaseClip) -> bool {
    clip.data
        .as_ref()
        .is_ok_and(|d| d.foot_labels.iter().any(|l| is_footfall(l)))
}

pub fn render(games: &[GameCensus]) -> String {
    let mut out = String::new();
    out.push_str(
        "# Locomotion clip census\n\n\
         **Generated** by `cargo run --release -p byroredux-nif --example locomotion_census -- --write`\n\
         (PRD [character-movement](../prd/character-movement.md), MOT-DX-005). Do not edit by hand.\n\n\
         It reads the vanilla base-game, patch and DLC archives of each installed game. It holds file\n\
         names, labels, counts and one rounded speed per base-set gait, and no keys, curves or\n\
         timings (MOT-SUP-007). A role comes from a clip's file name, so it says what is named, not\n\
         what a behaviour graph plays. \"Read\" means the engine's own decoder accepts the clip.\n\n\
         ## Archives read\n\n| Game | Archives | Humanoid skeleton |\n|---|---|---|\n",
    );
    for census in games {
        let list: Vec<String> = census.archives.iter().map(|a| format!("`{a}`")).collect();
        let skeleton = match census.skeleton {
            Some(true) => "decoded",
            Some(false) => "not decoded: COM tracks unchecked",
            None => "not needed",
        };
        let _ = writeln!(
            out,
            "| {} | {} | {skeleton} |",
            census.game.label(),
            list.join(", ")
        );
    }

    out.push_str("\n## Base set by role\n\n| Game | Base set |");
    for role in Role::ALL {
        let _ = write!(out, " {} |", role.label());
    }
    out.push_str(" no role |\n|---|---|");
    out.push_str(&"---|".repeat(Role::ALL.len() + 1));
    out.push('\n');
    for census in games {
        let _ = write!(
            out,
            "| {} | {} |",
            census.game.label(),
            base_set_description(census.game)
        );
        for role in Role::ALL {
            let _ = write!(out, " {} |", clips_where(census, Some(role), |_| true));
        }
        let _ = writeln!(out, " {} |", census.base_unroled);
    }

    out.push_str(
        "\n## Gait cycles: what each carries\n\n\
         A footfall label names a foot's step; a sync label alone does not count.\n\n\
         | Game | Read | With a footfall label | With a displacement source | With a `Blend` key |\n\
         |---|---|---|---|---|\n",
    );
    for census in games {
        let gaits = clips_where(census, Some(Role::Gait), |_| true);
        let count = |test: &dyn Fn(&BaseClip) -> bool| {
            share(clips_where(census, Some(Role::Gait), test), gaits)
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            census.game.label(),
            count(&|c| c.data.is_ok()),
            count(&has_footfall),
            count(&|c| c
                .data
                .as_ref()
                .is_ok_and(|d| d.displacement != Displacement::NoneRead)),
            count(&|c| c
                .data
                .as_ref()
                .is_ok_and(|d| d.other_labels.iter().any(|l| l.starts_with("Blend")))),
        );
    }

    out.push_str(
        "\n## All base-set locomotion clips: displacement source\n\n\
         A source counts when the clip's net horizontal travel is at least one game unit. A small travel\n\
         on a root or COM track may be a shift of weight and not locomotion.\n\n| Game | Role |",
    );
    let kinds = [
        Displacement::AccumulationRoot,
        Displacement::ExtractedMotion,
        Displacement::BoneTrack,
        Displacement::NoneRead,
    ];
    for kind in kinds {
        let _ = write!(out, " {} |", kind.label());
    }
    out.push_str(" not read | with a footfall label |\n|---|---|---|---|---|---|---|---|\n");
    for census in games {
        for role in Role::ALL {
            if clips_where(census, Some(role), |_| true) == 0 {
                continue;
            }
            let _ = write!(out, "| {} | {} |", census.game.label(), role.label());
            for kind in kinds {
                let n = clips_where(census, Some(role), |c| {
                    c.data.as_ref().is_ok_and(|d| d.displacement == kind)
                });
                let _ = write!(out, " {n} |");
            }
            let _ = writeln!(
                out,
                " {} | {} |",
                clips_where(census, Some(role), |c| c.data.is_err()),
                clips_where(census, Some(role), has_footfall),
            );
        }
    }

    out.push_str(
        "\n## Base-set clips one by one\n\n\
         Clips the engine can read. A speed is given for gait cycles only: the horizontal travel of the\n\
         displacement source over the clip's length, in game units a second, before any playback-rate\n\
         scale.\n\n\
         | Game | Clip | Role | Speed | Displacement | Footfall and sync labels |\n|---|---|---|---|---|---|\n",
    );
    for census in games {
        for clip in &census.base {
            let Ok(data) = &clip.data else { continue };
            let speed = data.speed.filter(|_| clip.role == Role::Gait);
            let speed = speed.map_or_else(|| "-".to_owned(), |s| s.to_string());
            let feet = if data.foot_labels.is_empty() {
                "-".to_owned()
            } else {
                data.foot_labels.join(", ")
            };
            let _ = writeln!(
                out,
                "| {} | `{}` | {} | {speed} | {} | {feet} |",
                census.game.label(),
                clip.name,
                clip.role.label(),
                data.displacement.label(),
            );
        }
    }

    out.push_str(
        "\n## Other labels on base-set locomotion clips\n\n\
         Digits are shown as `N`. The count is the number of clips that carry the label.\n\n\
         | Game | Labels |\n|---|---|\n",
    );
    for census in games {
        let mut labels: BTreeMap<&str, usize> = BTreeMap::new();
        for data in census.base.iter().filter_map(|c| c.data.as_ref().ok()) {
            for label in &data.other_labels {
                *labels.entry(label).or_default() += 1;
            }
        }
        let list: Vec<String> = labels
            .iter()
            .map(|(l, n)| format!("`{}` {n}", l.replace('|', "\\|")))
            .collect();
        let cell = if list.is_empty() {
            "-".to_owned()
        } else {
            list.join(", ")
        };
        let _ = writeln!(out, "| {} | {cell} |", census.game.label());
    }

    out.push_str(
        "\n## Every actor clip: what the engine reads\n\n\
         | Game | Humanoid clips read | Other actor clips read |\n|---|---|---|\n",
    );
    for census in games {
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            census.game.label(),
            share(census.humanoid.read, census.humanoid.total),
            share(census.other_actors.read, census.other_actors.total),
        );
    }
    out.push_str("\n| Game | Why a clip was not read | Clips |\n|---|---|---|\n");
    for census in games {
        for (reason, count) in &census.refusals {
            let _ = writeln!(
                out,
                "| {} | {} | {count} |",
                census.game.label(),
                reason.replace('|', "\\|")
            );
        }
    }

    out.push_str(
        "\n## Slope and stair names\n\n\
         Actor clips whose path names a slope, uphill, downhill, incline or stair.\n\n\
         | Game | Matching clips |\n|---|---|\n",
    );
    for census in games {
        let cell = if census.grade_names.is_empty() {
            "none".to_owned()
        } else {
            census
                .grade_names
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = writeln!(out, "| {} | {cell} |", census.game.label());
    }

    out.push_str(
        "\n## Climbing names\n\n\
         Actor clips whose path names a climb, vault, mantle, ladder or traversal, or going over or\n\
         up a wall. A name match is a candidate, not proof that the clip climbs.\n",
    );
    for census in games {
        let total: usize = census.climb_names.values().map(|v| v.len()).sum();
        let _ = writeln!(out, "\n### {} ({total})\n", census.game.label());
        if total == 0 {
            out.push_str("None.\n");
            continue;
        }
        out.push_str("| Directory | Clips |\n|---|---|\n");
        for (directory, clips) in &census.climb_names {
            let list: Vec<String> = clips.iter().map(|c| format!("`{c}`")).collect();
            let _ = writeln!(out, "| `{directory}` | {} |", list.join(", "));
        }
    }
    out
}
