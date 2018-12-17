// Copyright 2018 Google LLC, licensed under http://www.apache.org/licenses/LICENSE-2.0

use crate::{text, GfxCtx, Text, UserInput};
use geom::{Bounds, Pt2D};
use graphics::Transformed;
use opengl_graphics::{Filter, GlyphCache, TextureSettings};
use piston::window::Size;
use std::cell::RefCell;

const ZOOM_SPEED: f64 = 0.1;

pub struct Canvas {
    // All of these f64's are in screen-space, so do NOT use Pt2D.
    // Public for saving/loading... should probably do better
    pub cam_x: f64,
    pub cam_y: f64,
    pub cam_zoom: f64,

    cursor_x: f64,
    cursor_y: f64,

    left_mouse_drag_from: Option<(f64, f64)>,

    pub window_size: Size,

    glyphs: RefCell<GlyphCache<'static>>,
}

impl Canvas {
    pub fn new() -> Canvas {
        let texture_settings = TextureSettings::new().filter(Filter::Nearest);
        // TODO We could also preload everything and not need the RefCell.
        let glyphs = RefCell::new(
            GlyphCache::new(
                // TODO don't assume this exists!
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                (),
                texture_settings,
            )
            .expect("Could not load font"),
        );

        Canvas {
            cam_x: 0.0,
            cam_y: 0.0,
            cam_zoom: 1.0,

            cursor_x: 0.0,
            cursor_y: 0.0,

            left_mouse_drag_from: None,
            window_size: Size {
                width: 0,
                height: 0,
            },

            glyphs,
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.left_mouse_drag_from.is_some()
    }

    pub fn handle_event(&mut self, input: &mut UserInput) {
        if let Some((m_x, m_y)) = input.get_moved_mouse() {
            self.cursor_x = m_x;
            self.cursor_y = m_y;

            if let Some((click_x, click_y)) = self.left_mouse_drag_from {
                self.cam_x += click_x - m_x;
                self.cam_y += click_y - m_y;
                self.left_mouse_drag_from = Some((m_x, m_y));
            }
        }
        if input.left_mouse_button_pressed() {
            self.left_mouse_drag_from = Some((self.cursor_x, self.cursor_y));
        }
        if input.left_mouse_button_released() {
            self.left_mouse_drag_from = None;
        }
        if let Some(scroll) = input.get_mouse_scroll() {
            // Zoom slower at low zooms, faster at high.
            let delta = scroll * ZOOM_SPEED * self.cam_zoom;
            self.zoom_towards_mouse(delta);
        }
    }

    pub(crate) fn start_drawing(&mut self, g: &mut GfxCtx, window_size: Size) {
        self.window_size = window_size;
        g.ctx = g
            .orig_ctx
            .trans(-self.cam_x, -self.cam_y)
            .zoom(self.cam_zoom)
    }

    pub fn draw_mouse_tooltip(&self, g: &mut GfxCtx, txt: Text) {
        let glyphs = &mut self.glyphs.borrow_mut();
        let (width, height) = txt.dims(glyphs);
        let x1 = self.cursor_x - (width / 2.0);
        let y1 = self.cursor_y - (height / 2.0);
        text::draw_text_bubble(g, glyphs, (x1, y1), txt);
    }

    pub fn draw_text_at(&self, g: &mut GfxCtx, txt: Text, pt: Pt2D) {
        let glyphs = &mut self.glyphs.borrow_mut();
        let (width, height) = txt.dims(glyphs);
        let (x, y) = self.map_to_screen(pt);
        text::draw_text_bubble(g, glyphs, (x - (width / 2.0), y - (height / 2.0)), txt);
    }

    pub fn draw_text_at_topleft(&self, g: &mut GfxCtx, txt: Text, pt: Pt2D) {
        let (x, y) = self.map_to_screen(pt);
        text::draw_text_bubble(g, &mut self.glyphs.borrow_mut(), (x, y), txt);
    }

    pub fn draw_text_at_screenspace_topleft(&self, g: &mut GfxCtx, txt: Text, (x, y): (f64, f64)) {
        text::draw_text_bubble(g, &mut self.glyphs.borrow_mut(), (x, y), txt);
    }

    pub fn draw_text(
        &self,
        g: &mut GfxCtx,
        txt: Text,
        (horiz, vert): (HorizontalAlignment, VerticalAlignment),
    ) {
        if txt.is_empty() {
            return;
        }
        let glyphs = &mut self.glyphs.borrow_mut();
        let (width, height) = txt.dims(glyphs);
        let x1 = match horiz {
            HorizontalAlignment::Left => 0.0,
            HorizontalAlignment::Center => (f64::from(self.window_size.width) - width) / 2.0,
            HorizontalAlignment::Right => f64::from(self.window_size.width) - width,
        };
        let y1 = match vert {
            VerticalAlignment::Top => 0.0,
            VerticalAlignment::Center => (f64::from(self.window_size.height) - height) / 2.0,
            VerticalAlignment::Bottom => f64::from(self.window_size.height) - height,
        };
        text::draw_text_bubble(g, glyphs, (x1, y1), txt);
    }

    pub(crate) fn text_dims(&self, txt: &Text) -> (f64, f64) {
        txt.dims(&mut self.glyphs.borrow_mut())
    }

    fn zoom_towards_mouse(&mut self, delta_zoom: f64) {
        let old_zoom = self.cam_zoom;
        self.cam_zoom += delta_zoom;
        if self.cam_zoom <= ZOOM_SPEED {
            self.cam_zoom = ZOOM_SPEED;
        }

        // Make screen_to_map of cursor_{x,y} still point to the same thing after zooming.
        self.cam_x = ((self.cam_zoom / old_zoom) * (self.cursor_x + self.cam_x)) - self.cursor_x;
        self.cam_y = ((self.cam_zoom / old_zoom) * (self.cursor_y + self.cam_y)) - self.cursor_y;
    }

    pub fn get_cursor_in_map_space(&self) -> Pt2D {
        self.screen_to_map((self.cursor_x, self.cursor_y))
    }

    pub fn screen_to_map(&self, (x, y): (f64, f64)) -> Pt2D {
        Pt2D::new(
            (x + self.cam_x) / self.cam_zoom,
            (y + self.cam_y) / self.cam_zoom,
        )
    }

    pub fn center_to_map_pt(&self) -> Pt2D {
        self.screen_to_map((
            f64::from(self.window_size.width) / 2.0,
            f64::from(self.window_size.height) / 2.0,
        ))
    }

    pub fn center_on_map_pt(&mut self, pt: Pt2D) {
        self.cam_x = (pt.x() * self.cam_zoom) - (f64::from(self.window_size.width) / 2.0);
        self.cam_y = (pt.y() * self.cam_zoom) - (f64::from(self.window_size.height) / 2.0);
    }

    fn map_to_screen(&self, pt: Pt2D) -> (f64, f64) {
        (
            (pt.x() * self.cam_zoom) - self.cam_x,
            (pt.y() * self.cam_zoom) - self.cam_y,
        )
    }

    pub fn get_screen_bounds(&self) -> Bounds {
        let mut b = Bounds::new();
        b.update(self.screen_to_map((0.0, 0.0)));
        b.update(self.screen_to_map((
            f64::from(self.window_size.width),
            f64::from(self.window_size.height),
        )));
        b
    }
}

pub enum HorizontalAlignment {
    Left,
    Center,
    Right,
}

pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

pub const BOTTOM_LEFT: (HorizontalAlignment, VerticalAlignment) =
    (HorizontalAlignment::Left, VerticalAlignment::Bottom);
pub const TOP_RIGHT: (HorizontalAlignment, VerticalAlignment) =
    (HorizontalAlignment::Right, VerticalAlignment::Top);
pub const CENTERED: (HorizontalAlignment, VerticalAlignment) =
    (HorizontalAlignment::Center, VerticalAlignment::Center);
