//! A 2D "world" you can put things in and drag them around: radar targets, antennas,
//! walls, mesh nodes. World coordinates are in whatever unit the demo likes (metres,
//! grid cells); the scene maps them onto the screen keeping the proportions.

use crate::theme;
use egui::{Id, Pos2, Rect, Response, Sense, Ui, Vec2, emath::RectTransform};
use std::hash::Hash;

pub struct SceneUi<'u> {
    pub ui: &'u mut Ui,
    pub painter: egui::Painter,
    /// Screen rectangle the world is drawn into.
    pub rect: Rect,
    pub world: Rect,
    /// Response of the background (clicks and drags that hit no handle).
    pub background: Response,
    to_screen: RectTransform,
    id: Id,
}

impl SceneUi<'_> {
    /// World position to screen position.
    pub fn pos(&self, world: Pos2) -> Pos2 {
        self.to_screen.transform_pos(world)
    }

    /// World length to screen pixels.
    pub fn len(&self, world: f32) -> f32 {
        world * self.to_screen.scale().x
    }

    pub fn to_world(&self, screen: Pos2) -> Pos2 {
        self.to_screen.inverse().transform_pos(screen)
    }

    /// World position of a click on empty background this frame.
    pub fn background_click(&self) -> Option<Pos2> {
        if self.background.clicked() { self.background.interact_pointer_pos().map(|p| self.to_world(p)) } else { None }
    }

    /// A draggable point. `key` must be stable for the object (not its list index if
    /// objects can be removed). Moves `world_pos` while dragged and keeps it inside
    /// the world. Returns the response for hover, click and context menus.
    pub fn handle(&mut self, key: impl Hash + std::fmt::Debug, world_pos: &mut Pos2, radius_px: f32) -> Response {
        let centre = self.pos(*world_pos);
        let hit = Rect::from_center_size(centre, Vec2::splat(2.0 * radius_px.max(9.0)));
        let response = self.ui.interact(hit, self.id.with(key), Sense::click_and_drag());
        if response.dragged() {
            let delta = response.drag_delta() / self.to_screen.scale();
            *world_pos = self.world.clamp(*world_pos + delta);
        }
        if response.hovered() || response.dragged() {
            self.ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        response
    }
}

/// Show a scene. `max_height` limits how tall it may get; the world keeps its
/// proportions and is as wide as the available space allows.
pub fn scene<R>(ui: &mut Ui, id_salt: &str, world: Rect, max_height: f32, add: impl FnOnce(&mut SceneUi<'_>) -> R) -> R {
    let pal = theme::palette(ui);
    let aspect = world.width() / world.height();
    let mut width = ui.available_width().max(120.0);
    let mut height = width / aspect;
    if height > max_height {
        height = max_height;
        width = height * aspect;
    }
    let (rect, background) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, pal.canvas);
    let id = ui.make_persistent_id(id_salt);
    let mut scene_ui = SceneUi { ui, painter, rect, world, background, to_screen: RectTransform::from_to(world, rect), id };
    let out = add(&mut scene_ui);
    scene_ui.painter.rect_stroke(rect, 4.0, (1.0, pal.grid), egui::StrokeKind::Inside);
    out
}
