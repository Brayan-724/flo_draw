use flo_render_software::render::*;

use flo_render_software::canvas::*;

use futures::prelude::*;
use futures::stream;
use futures::executor;

use std::sync::*;
use std::time::{Instant};

///
/// Render some text using the canvas's text-to-outline converer from flo_canvas
///
pub fn main() {
    // Load the Lato font that we'll use for the test
    let lato = CanvasFontFace::from_slice(include_bytes!("../test_data/Lato-Regular.ttf"));

    // Create some drawing commands to render the text
    let mut gc = vec![];

    gc.clear_canvas(Color::Rgba(1.0, 1.0, 1.0, 1.0));
    gc.canvas_height(1080.0);
    gc.center_region(0.0, 0.0, 1920.0, 1080.0);

    gc.define_font_data(FontId(0), Arc::clone(&lato));

    // Lay out some text in the graphics context
    gc.set_font_size(FontId(0), 24.0);
    gc.draw_text(FontId(0), "Rendering text with the software renderer".to_string(), 64.0, 1080.0 - 24.0 - 64.0);

    // Convert the font instructions to 'normal' drawing instructions (bypassing any renderer that might be added by the software renderer)
    let drawing = stream::iter(gc);
    let drawing = drawing_with_laid_out_text(drawing);
    let drawing = drawing_with_text_as_paths(drawing);
    let drawing = executor::block_on(async move { drawing.collect::<Vec<_>>().await });

    // Render to the terminal window
    render_drawing(&mut TerminalRenderTarget::new(1920, 1080), drawing.iter().cloned());
}
