use super::pixel_filter_trait::*;
use crate::pixel::*;

use std::sync::*;
use std::marker::{PhantomData};

///
/// The mask filter multiplies the output pixels by the contents of a mask texture
///
pub struct MaskFilter<TPixel, const N: usize>
where
    TPixel: Pixel<N>,
{
    mask:   Arc<U16LinearTexture>,
    mult_x: f64,
    mult_y: f64,
    pixel:  PhantomData<TPixel>,
}

impl<TPixel, const N: usize> MaskFilter<TPixel, N> 
where
    TPixel: Pixel<N>,
{
    ///
    /// Creates a new mask filter that will use the alpha value from the specified texture to mask the input texture
    ///
    pub fn with_mask(mask: &Arc<U16LinearTexture>, multiply_x: f64, multiply_y: f64) -> Self {
        MaskFilter {
            mask:   Arc::clone(mask),
            mult_x: multiply_x,
            mult_y: multiply_y,
            pixel:  PhantomData,
        }
    }

    ///
    /// Reads the red and green fraction of the pixels given the lower and upper lines, x position and y fraction
    ///
    #[inline]
    fn read_px(&self, xpos: usize, line_pixels_1: &[U16LinearPixel], line_pixels_2: &[U16LinearPixel], ypos_fract: u32) -> u16 {
        // Calculate the x position along the lines by multiplying by the map position
        let xpos        = xpos as f64 * self.mult_x;
        let xpos        = xpos.abs() % line_pixels_1.len() as f64;
        let xpos_fract  = xpos.fract();
        let xpos_fract  = (xpos_fract * 65535.0) as u32;
        let xpos        = xpos as usize;
        let xpos_1      = (xpos+1) % line_pixels_1.len();

        // Read the 4 corners of the pixel
        let px1 = line_pixels_1[xpos];
        let px2 = line_pixels_1[xpos_1];
        let px3 = line_pixels_2[xpos];
        let px4 = line_pixels_2[xpos];

        // Only need the alpha channel: calculate the value using bilinear filtering
        let a1 = px1.a() as u32;
        let a2 = px2.a() as u32;
        let a3 = px3.a() as u32;
        let a4 = px4.a() as u32;

        let a12 = ((a2 * xpos_fract)>>16) + ((a1 * (65535-xpos_fract))>>16);
        let a34 = ((a4 * xpos_fract)>>16) + ((a3 * (65535-xpos_fract))>>16);

        let a = ((a34 * ypos_fract)>>16) + ((a12 * (65535-ypos_fract))>>16);

        a as u16
    }
}

impl<TPixel, const N: usize> PixelFilter for MaskFilter<TPixel, N> 
where
    TPixel: Pixel<N>,
{
    type Pixel = TPixel;

    #[inline]
    fn with_scale(&self, _x_scale: f64, _y_scale: f64) -> Option<Arc<dyn Send + Sync + PixelFilter<Pixel=Self::Pixel>>> {
        None
    }

    #[inline]
    fn input_lines(&self) -> (usize, usize) {
        (0, 0)
    }

    #[inline]
    fn extra_columns(&self) -> (usize, usize) {
        (0, 0)
    }

    fn filter_line(&self, y_pos: usize, input_lines: &[&[Self::Pixel]], output_line: &mut [Self::Pixel]) {
        // Read two lines from the mask (for bilinear filtering)
        let mask_y          = (y_pos as f64) * self.mult_y;
        let mask_y_fract    = mask_y.abs().fract();
        let mask_y          = mask_y.abs() as usize;
        let mask_y_fract    = (mask_y_fract * 65535.0) as u32;

        let mask_line_1     = self.mask.pixel_line(mask_y);
        let mask_line_2     = self.mask.pixel_line(mask_y+1);

        if let (Some(mask_line_1), Some(mask_line_2)) = (mask_line_1, mask_line_2) {
            let mask_line_1 = U16LinearPixel::u16_slice_as_linear_pixels_immutable(mask_line_1);
            let mask_line_2 = U16LinearPixel::u16_slice_as_linear_pixels_immutable(mask_line_2);

            // Read from the mask for each input pixel
            for (x_pos, (input_px, output_px)) in input_lines[0].iter().zip(output_line.iter_mut()).enumerate() {
                // Read the alpha value from the mask at this position
                let mask_alpha = self.read_px(x_pos, mask_line_1, mask_line_2, mask_y_fract);
                let mask_alpha = (mask_alpha as f64) / 65535.0;
                let mask_alpha = TPixel::Component::with_value(mask_alpha);

                *output_px = *input_px * mask_alpha;
            }
        }
    }
}