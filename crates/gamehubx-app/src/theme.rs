use eframe::egui::{Color32, Frame};

pub(crate) const BG: Color32 = Color32::from_rgb(32, 36, 47);
pub(crate) const CARD: Color32 = Color32::from_rgb(45, 50, 61);
pub(crate) const DIM: Color32 = Color32::from_rgb(180, 180, 180);
pub(crate) const ACC: Color32 = Color32::from_rgb(77, 143, 255);
pub(crate) const GREEN: Color32 = Color32::from_rgb(73, 217, 171);
pub(crate) const W: Color32 = Color32::WHITE;
pub(crate) const CLEAR: Color32 = Color32::TRANSPARENT;

pub(crate) fn frame() -> Frame {
    Frame::none()
        .fill(Color32::from_rgb(34, 38, 46))
        .inner_margin(30.0)
}
