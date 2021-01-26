//!
//! `flo_draw` provides a simple API for rendering 2D graphics
//!

#[macro_use] extern crate lazy_static;

pub use flo_canvas as canvas;

mod canvas_window;
mod render_window;
mod glutin_thread;
mod glutin_thread_event;

pub use self::canvas_window::*;
pub use self::render_window::*;
