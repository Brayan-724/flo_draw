use std::{ops::{Range}, marker::PhantomData};

///
/// A pixel program descibes how to draw pixels along a scan line
///
pub trait PixelProgram : Send {
    /// Data associated with a particular instance of this program
    type ProgramData;

    /// Data for the individual scanlines for this program
    type ScanlineData;

    ///
    /// Draws a series of pixels to a frame buffer
    ///
    /// The target points to the start of the range of values to be written. `x_range` provides the range of X values to fill with pixels.
    ///
    fn draw_pixels(&self, target: &mut [[f32; 4]], x_range: Range<i32>, y_pos: i32, program_data: &Self::ProgramData, scanline_data: &Self::ScanlineData);

    ///
    /// Returns the data for the scanlines the program will be run over
    ///
    /// Scanlines generally are taken over a contiguous range, starting at `min_y`. 
    ///
    fn create_scanline_data(&self, min_y: i32, scanlines: &Vec<PixelProgramScanline>, program_data: &Self::ProgramData) -> Self::ScanlineData;
}

///
/// Describes a scanline when creating the scanline data
///
pub struct PixelProgramScanline {
    /// The exact range of values intercepted by the program, before dealing with any clipping or occlusion
    pub x_range: Range<f32>,

    /// The y position of this scanline
    pub y_pos: f32,
}

///
/// Pixel program that calls a function to fill the pixels, with program data
///
/// This can be used with a pixel program that generates rows of pixels (`PixelProgramFn::from(|target, x_range, ypos, data| { ... })`)
///
pub struct PixelProgramFn<TFn, TData>
where 
    TFn: Send + Fn(&mut [[f32; 4]], Range<i32>, i32, &TData) -> (),
{
    /// The function to call to fill in the pixels
    function: TFn,

    /// Placeholder for the TData type (Rust doesn't see a function parameter as a constraint)
    phantom_data: PhantomData<TData>,
}

///
/// Pixel program that calls a function to fill the pixels, with program data
///
/// This can be used with a pixel program that generates individual pixels (`PerPixelProgramFn::from(|x, y, data| { [r, g, b, a] })`)
///
pub struct PerPixelProgramFn<TFn, TData>
where 
    TFn: Fn(i32, i32, &TData) -> [f32; 4],
{
    /// The function to call to fill in the pixels
    function: TFn,

    /// Placeholder for the TData type (Rust doesn't see a function parameter as a constraint)
    phantom_data: PhantomData<TData>,
}

///
/// Simple functions can be pixel programs that take no program data
///
impl<TFn> PixelProgram for TFn
where
    TFn: Send + Fn(&mut [[f32; 4]], Range<i32>, i32, &()) -> (),
{
    type ProgramData    = ();
    type ScanlineData   = ();

    #[inline]
    fn draw_pixels(&self, target: &mut [[f32; 4]], x_range: Range<i32>, ypos: i32, program_data: &(), _scanline_data: &()) {
        (*self)(target, x_range, ypos, program_data)
    }

    #[inline]
    fn create_scanline_data(&self, _min_y: i32, _scanlines: &Vec<PixelProgramScanline>, _program_data: &Self::ProgramData) -> () {
        ()
    }
}

impl<TFn, TData> From<TFn> for PixelProgramFn<TFn, TData> 
where 
    TFn: Send + Fn(&mut [[f32; 4]], Range<i32>, i32, &TData) -> (),
{
    fn from(function: TFn) -> Self {
        PixelProgramFn {
            function:       function,
            phantom_data:   PhantomData,
        }
    }
}

impl<TFn, TData> PixelProgram for PixelProgramFn<TFn, TData> 
where 
    TFn:    Send + Fn(&mut [[f32; 4]], Range<i32>, i32, &TData) -> (),
    TData:  Send,
{
    type ProgramData    = TData;
    type ScanlineData   = ();

    #[inline]
    fn draw_pixels(&self, target: &mut [[f32; 4]], x_range: Range<i32>, ypos: i32, program_data: &TData, _scanline_data: &()) {
        (self.function)(target, x_range, ypos, program_data)
    }

    #[inline]
    fn create_scanline_data(&self, _min_y: i32, _scanlines: &Vec<PixelProgramScanline>, _program_data: &Self::ProgramData) -> () {
        ()
    }
}

impl<TFn, TData> From<TFn> for PerPixelProgramFn<TFn, TData> 
where 
    TFn: Fn(i32, i32, &TData) -> [f32; 4],
{
    fn from(function: TFn) -> Self {
        PerPixelProgramFn {
            function:       function,
            phantom_data:   PhantomData,
        }
    }
}

impl<TFn, TData> PixelProgram for PerPixelProgramFn<TFn, TData> 
where 
    TFn:    Send + Fn(i32, i32, &TData) -> [f32; 4],
    TData:  Send,
{
    type ProgramData    = TData;
    type ScanlineData   = ();

    #[inline]
    fn draw_pixels(&self, target: &mut [[f32; 4]], x_range: Range<i32>, ypos: i32, program_data: &TData, _scanline_data: &()) {
        let mut pos = 0;
        for x in x_range {
            target[pos] = (self.function)(x, ypos, program_data);
            pos += 1;
        }
    }

    #[inline]
    fn create_scanline_data(&self, _min_y: i32, _scanlines: &Vec<PixelProgramScanline>, _program_data: &Self::ProgramData) -> () {
        ()
    }
}

///
/// Identifier for a pixel program
///
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PixelProgramId(pub (crate) usize);
