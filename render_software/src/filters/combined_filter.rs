use super::pixel_filter_trait::*;
use crate::pixel::*;

use std::sync::*;
use std::marker::{PhantomData};

///
/// The mask filter multiplies the output pixels by the contents of a mask texture
///
pub struct CombinedFilter<TPixel, const N: usize>
where
    TPixel: Pixel<N>,
{
    /// The filters that are being copmbined
    filters: Vec<Arc<dyn Send + Sync + PixelFilter<Pixel=TPixel>>>,
}

impl<TPixel, const N: usize> CombinedFilter<TPixel, N>
where
    TPixel: Pixel<N>,
{
    ///
    /// Creates a combined filter from a set of input filters (all the input filters will be applied to the result)
    ///
    pub fn from_filter(filters: impl IntoIterator<Item=Arc<dyn Send + Sync + PixelFilter<Pixel=TPixel>>>) -> Self {
        CombinedFilter {
            filters: filters.into_iter().collect(),
        }
    }
}

impl<TPixel, const N: usize> PixelFilter for CombinedFilter<TPixel, N>
where
    TPixel: Pixel<N>,
{
    type Pixel = TPixel;

    fn input_lines(&self) -> (usize, usize) {
        // The combined value is found by summing the requested values from all of the filters we're combining
        let mut top     = 0;
        let mut bottom  = 0;

        for filter in self.filters.iter() {
            let (filter_top, filter_bottom) = filter.input_lines();
            top     = filter_top + top;
            bottom  = filter_bottom + bottom;
        }

        (top, bottom)
    }

    fn extra_columns(&self) -> (usize, usize) {
        // The combined value is found by summing the requested values from all of the filters we're combining
        let mut left    = 0;
        let mut right   = 0;

        for filter in self.filters.iter() {
            let (filter_left, filter_right) = filter.extra_columns();
            left    = filter_left + left;
            right   = filter_right + right;
        }

        (left, right)
    }

    fn filter_line(&self, y_pos: usize, input_lines: &[&[Self::Pixel]], output_line: &mut [Self::Pixel]) {
        if self.filters.len() == 0 {
            // Edge case: no filters = copy the input to the output
            for (input, output) in input_lines[0].iter().zip(output_line.iter_mut()) {
                *output = *input;
            }
        } else if self.filters.len() == 1 {
            // Edge case: just call the first filter directly
            self.filters[0].filter_line(y_pos, input_lines, output_line)
        } else {
            // Apply each filter in turn to generate the input for the next filter along
            todo!()
        }
    }
}