use super::scanline_data::*;

use crate::pixel::*;
use crate::scanplan::*;
use crate::filters::*;

use std::ops::{Range};
use std::marker::{PhantomData};

///
/// Applies a filter to the result of rendering a scanline program
///
/// The full rendering is only applied to the region of the scanlines that are actually rendered on screen
///
pub struct FilteredScanlineProgram<TFilter, TPixel, const N: usize>
where
    TPixel: Pixel<N>,
{
    /// The filter to apply to the output of the scanline program
    filter: TFilter,

    // Pixel data
    pixel: PhantomData<TPixel>,
}

impl<TFilter, TPixel, const N: usize> PixelProgram for FilteredScanlineProgram<TFilter, TPixel, N>
where
    TPixel: Pixel<N>,
    TFilter: Send + Sync + PixelFilter<Pixel = TPixel>,
{
    type Pixel          = TPixel;
    type ProgramData    = ();

    #[inline]
    fn draw_pixels(&self, _data_cache: &PixelProgramRenderCache<Self::Pixel>, target: &mut [Self::Pixel], pixel_range: Range<i32>, x_transform: &ScanlineTransform, y_pos: f64, data: &Self::ProgramData) {
        // todo
    }
}
