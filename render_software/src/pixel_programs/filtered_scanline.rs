use crate::edgeplan::*;
use crate::filters::*;
use crate::pixel::*;
use crate::scanplan::*;

use std::collections::{HashMap};
use std::ops::{Range};
use std::sync::*;
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


///
/// Data that can be used to run a basic sprite program
///
pub struct FilteredScanlineData<TEdgeDescriptor>
where
    TEdgeDescriptor: EdgeDescriptor,
{
    /// The edges that will be used to generate the scanplan for this program
    edges: Arc<EdgePlan<TEdgeDescriptor>>,

    /// The scaling to apply to coordinates supplied to the edge plan
    scale: (f64, f64),

    /// The translation to apply to coordinates supplied to the edge plan
    translate: (f64, f64),

    /// The scanline plan for each y-position (updated for new scanlines)
    scanlines: RwLock<HashMap<f64, Arc<ScanlinePlan>>>,
}

impl<TFilter, TPixel, const N: usize> PixelProgram for FilteredScanlineProgram<TFilter, TPixel, N>
where
    TPixel: Pixel<N>,
    TFilter: Send + Sync + PixelFilter<Pixel = TPixel>,
{
    type Pixel          = TPixel;
    type ProgramData    = ();

    #[inline]
    fn draw_pixels(&self, data_cache: &PixelProgramRenderCache<Self::Pixel>, target: &mut [Self::Pixel], pixel_range: Range<i32>, x_transform: &ScanlineTransform, y_pos: f64, data: &Self::ProgramData) {
        // todo
    }
}
