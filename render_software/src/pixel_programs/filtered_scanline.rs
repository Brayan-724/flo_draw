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
pub struct FilteredScanlineProgram<TEdgeDescriptor, TFilter, TPlanner, TPixel, const N: usize>
where
    TPixel: Pixel<N>,
{
    /// The filter to apply to the output of the scanline program
    filter: PhantomData<TFilter>,

    // Pixel data
    pixel: PhantomData<TPixel>,

    // Edge descriptor data
    edge: PhantomData<TEdgeDescriptor>,

    /// The pixel planner
    planner: TPlanner,
}

///
/// Data that can be used to run a basic sprite program
///
pub struct FilteredScanlineData<TEdgeDescriptor, TFilter>
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
    scanlines: RwLock<HashMap<u64, Arc<ScanlinePlan>>>,

    /// The filter to apply to the pixels generated from the scanlines
    filter: TFilter,
}

impl<TEdgeDescriptor, TFilter, TPlanner, TPixel, const N: usize> PixelProgram for FilteredScanlineProgram<TEdgeDescriptor, TFilter, TPlanner, TPixel, N>
where
    TPixel:             Pixel<N>,
    TFilter:            Send + Sync + PixelFilter<Pixel = TPixel>,
    TEdgeDescriptor:    EdgeDescriptor,
    TPlanner:           Send + Sync + ScanPlanner<Edge=TEdgeDescriptor>,
{
    type Pixel          = TPixel;
    type ProgramData    = FilteredScanlineData<TEdgeDescriptor, TFilter>;

    #[inline]
    fn draw_pixels(&self, data_cache: &PixelProgramRenderCache<Self::Pixel>, target: &mut [Self::Pixel], pixel_range: Range<i32>, x_transform: &ScanlineTransform, y_pos: f64, data: &Self::ProgramData) {
        use std::mem;

        // Try to retrieve the scanline, or plan a new one if needed
        let scanline = {
            let scanlines = data.scanlines.read().unwrap();
            if let Some(existing_scanline) = scanlines.get(&y_pos.to_bits()) {
                // Re-use the previously calculated scanline
                Arc::clone(existing_scanline)
            } else {
                // Cache a new scanline to re-use
                mem::drop(scanlines);

                // Calculate the transform for the sprite region
                let scan_ypos       = y_pos * data.scale.1 + data.translate.1;
                let scan_transform  = x_transform.transform(data.scale.0, data.translate.0);
                let scan_xrange     = scan_transform.pixel_x_to_source_x(0)..scan_transform.pixel_x_to_source_x(x_transform.width_in_pixels() as _);

                // Plan the rendering for the sprite
                let mut new_scanline = [(scan_ypos, ScanlinePlan::default())];
                self.planner.plan_scanlines(&*data.edges, &scan_transform, &[scan_ypos], scan_xrange, &mut new_scanline);

                // Store as the new cached value
                let mut scanlines = data.scanlines.write().unwrap();

                let mut new_plan = ScanlinePlan::default();
                mem::swap(&mut new_scanline[0].1, &mut new_plan);
                let new_scanline = Arc::new(new_plan);

                scanlines.insert(y_pos.to_bits(), new_scanline.clone());

                new_scanline
            }
        };

        // todo
    }
}
