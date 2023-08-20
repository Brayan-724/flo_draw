use super::renderer::*;

use crate::pixel::*;

use std::marker::{PhantomData};

///
/// Renders a whole frame of pixels to a RGBA U8 buffer
///
pub struct U8FrameRenderer<TPixel, TRegionRenderer, const N: usize>
where
    TPixel:                         Send + Pixel<N>,
    for<'a> &'a TRegionRenderer:    Renderer<Source=[f64], Dest=[&'a mut [TPixel]]>,
{
    width:              usize,
    height:             usize,
    gamma:              f64,
    region_renderer:    TRegionRenderer,
    pixel:              PhantomData<TPixel>,
}


impl<TPixel, TRegionRenderer, const N: usize> U8FrameRenderer<TPixel, TRegionRenderer, N>
where
    TPixel:                         Send + Pixel<N>,
    for<'a> &'a TRegionRenderer:    Renderer<Source=[f64], Dest=[&'a mut [TPixel]]>,
{
    ///
    /// Creates a new frame renderer
    ///
    /// Use a gamma value of 2.2 for most rendering tasks (this is the default used by most operating systems)
    ///
    pub fn new(width: usize, height: usize, gamma: f64, region_renderer: TRegionRenderer) -> Self {
        Self {
            width:              width, 
            height:             height,
            gamma:              gamma,
            region_renderer:    region_renderer,
            pixel:              PhantomData,
        }
    }
}

impl<'a, TPixel, TRegionRenderer, const N: usize> Renderer for &'a U8FrameRenderer<TPixel, TRegionRenderer, N> 
where
    TPixel:                         Sized + Send + Default + Pixel<N>,
    for<'b> &'b TRegionRenderer:    Renderer<Source=[f64], Dest=[&'b mut [TPixel]]>,
{
    type Source = ();       // Source is '()' because the region renderer references the edge plan that is the 'true' source; TODO: supply the edge plan here?
    type Dest   = [U8RgbaPremultipliedPixel];

    fn render(&self, _source: &(), dest: &mut [U8RgbaPremultipliedPixel]) {
        const LINES_AT_ONCE: usize = 8;

        // Cut the destination into chunks to form the lines
        let mut chunks  = dest.chunks_exact_mut(self.width).collect::<Vec<_>>();
        let renderer    = &self.region_renderer;

        // Rendering fails if there are insufficient lines to complete
        if chunks.len() < self.height {
            panic!("Cannot render: needed an output buffer large enough to fit {} lines but found {} lines", self.height, chunks.len());
        }

        // Render in chunks of LINES_AT_ONCE lines
        let mut y_idx           = 0;
        let mut y_positions     = vec![];
        let mut buffer          = vec![TPixel::default(); self.width*LINES_AT_ONCE];
        let mut buffer_chunks   = buffer.chunks_exact_mut(self.width).collect::<Vec<_>>();
        loop {
            // Stop once we reach the end
            if y_idx >= self.height {
                break;
            }

            // Work out which lines to render next
            let start_idx   = y_idx;
            let end_idx     = start_idx + LINES_AT_ONCE;
            let end_idx     = if end_idx > self.height { self.height } else { end_idx };

            // Write the y positions
            y_positions.clear();
            y_positions.extend((start_idx..end_idx).map(|idx| idx as f64));

            // Render these lines
            renderer.render(&y_positions, &mut buffer_chunks);

            // Convert to the final pixel format
            for y_idx in 0..(end_idx-start_idx) {
                let rendered_pixels = &mut buffer_chunks[y_idx];
                let target_pixels   = &mut chunks[start_idx + y_idx];

                for x_idx in 0..self.width {
                    target_pixels[x_idx] = rendered_pixels[x_idx].to_u8_rgba(self.gamma);
                }
            }

            // Advance to the next y position
            y_idx = end_idx;
        }
    } 
}
