use crate::{
    svg, Drawable, EventCtx, GeomBatch, GfxCtx, RewriteColor, ScreenDims, ScreenPt, Widget,
    WidgetImpl, WidgetOutput,
};

// Just draw something. A widget just so widgetsing works.
pub struct JustDraw {
    pub(crate) draw: Drawable,

    pub(crate) top_left: ScreenPt,
    pub(crate) dims: ScreenDims,
}

impl JustDraw {
    pub(crate) fn wrap(ctx: &EventCtx, batch: GeomBatch) -> Widget {
        Widget::new(Box::new(JustDraw {
            dims: batch.get_dims(),
            draw: ctx.upload(batch),
            top_left: ScreenPt::new(0.0, 0.0),
        }))
    }

    pub(crate) fn svg(ctx: &EventCtx, filename: String) -> Widget {
        let (batch, bounds) = svg::load_svg(ctx.prerender, &filename);
        // TODO The dims will be wrong; it'll only look at geometry, not the padding in the image.
        Widget::new(Box::new(JustDraw {
            dims: ScreenDims::new(bounds.width(), bounds.height()),
            draw: ctx.upload(batch),
            top_left: ScreenPt::new(0.0, 0.0),
        }))
    }
    pub(crate) fn svg_transform(ctx: &EventCtx, filename: &str, rewrite: RewriteColor) -> Widget {
        let (mut batch, bounds) = svg::load_svg(ctx.prerender, filename);
        batch.rewrite_color(rewrite);
        // TODO The dims will be wrong; it'll only look at geometry, not the padding in the image.
        Widget::new(Box::new(JustDraw {
            dims: ScreenDims::new(bounds.width(), bounds.height()),
            draw: ctx.upload(batch),
            top_left: ScreenPt::new(0.0, 0.0),
        }))
    }
}

impl WidgetImpl for JustDraw {
    fn get_dims(&self) -> ScreenDims {
        self.dims
    }

    fn set_pos(&mut self, top_left: ScreenPt) {
        self.top_left = top_left;
    }

    fn event(&mut self, _ctx: &mut EventCtx, _output: &mut WidgetOutput) {}

    fn draw(&self, g: &mut GfxCtx) {
        g.redraw_at(self.top_left, &self.draw);
    }
}
