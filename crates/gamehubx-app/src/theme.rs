use eframe::egui::{Color32, Frame};

pub(crate) const BG: Color32 = Color32::from_rgb(7, 14, 32);
pub(crate) const CARD: Color32 = Color32::from_rgb(24, 30, 46);
pub(crate) const DIM: Color32 = Color32::from_rgb(140, 148, 165);
pub(crate) const ACC: Color32 = Color32::from_rgb(59, 130, 246);
pub(crate) const W: Color32 = Color32::WHITE;
pub(crate) const CLEAR: Color32 = Color32::TRANSPARENT;

pub(crate) fn frame() -> Frame {
    Frame::none()
        .fill(Color32::from_rgb(10, 18, 38))
        .inner_margin(30.0)
}
