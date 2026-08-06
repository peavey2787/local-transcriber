//! Shared typography and spacing for readable settings and overlay controls.

use egui::{self, FontId, TextStyle};

pub fn configure(context: &egui::Context) {
    let mut style = (*context.style()).clone();
    style.visuals = egui::Visuals::dark();
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(22.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.5));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(14.0));
    style.interaction.selectable_labels = false;
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.interact_size.y = 30.0;
    context.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_small_text_remains_readable() {
        let context = egui::Context::default();
        configure(&context);
        let style = context.style();
        assert_eq!(style.text_styles[&TextStyle::Small].size, 12.5);
        assert_eq!(style.text_styles[&TextStyle::Body].size, 15.0);
    }
}
