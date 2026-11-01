use super::*;

mod model;
mod placements;
mod shortcuts;
mod tags;
mod triggers;

use gpui::Keystroke;

fn keystroke(source: &str) -> Keystroke {
    match Keystroke::parse(source) {
        Ok(keystroke) => keystroke,
        Err(error) => panic!("{source} is not a keystroke: {error}"),
    }
}
