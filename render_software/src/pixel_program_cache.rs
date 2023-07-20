use crate::pixel_program::*;

use std::marker::{PhantomData};
use std::ops::{Range};
use std::sync::*;

///
/// The pixel program cache provides a way to assign IDs to pixel programs and support initialising them
/// with a data cache.
///
pub struct PixelProgramCache<TPixel> {
    next_program_id:    usize,
    phantom_data:       PhantomData<TPixel>,
}

///
/// The pixel program data cache stores the program data for the pixel programs
///
pub struct PixelProgramDataCache<TPixel> {
    /// Program data is encapsulated in a function that generates the scanline data. This is indexed by `PixelProgramDataId`
    program_data: Vec<Box<dyn Fn(i32, &Vec<PixelProgramScanline>) -> Box<dyn Fn(&mut [TPixel], Range<i32>, i32) -> ()>>>,

    /// The scanline_data functions encapsulate the program data and the scanline data indicate programs that are ready to run
    scanline_data: Vec<Box<dyn Fn(&mut [TPixel], Range<i32>, i32) -> ()>>,
}

///
/// A data manager is used to store data associated with a program into a data cache
///
pub struct StoredPixelProgram<TProgram>
where
    TProgram: 'static + PixelProgram,
{
    /// The ID of this pixel program
    program_id: PixelProgramId,

    /// Function to associate program data with this program
    associate_program_data: Box<dyn Fn(TProgram::ProgramData) -> Box<dyn Fn(i32, &Vec<PixelProgramScanline>) -> Box<dyn Fn(&mut [TProgram::Pixel], Range<i32>, i32) -> ()>>>,
}

///
/// Identifier for the program data for a pixel program
///
/// Every pixel program has a separate set of identifiers for their data
///
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PixelProgramDataId(usize);

///
/// Identifier for some scanline data
///
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PixelScanlineDataId(usize);

impl<TProgram> StoredPixelProgram<TProgram>
where
    TProgram: 'static + PixelProgram,
{
    /// Returns the program ID of this program
    #[inline]
    pub fn program_id(&self) -> PixelProgramId {
        self.program_id
    }
}

impl<TPixel> PixelProgramCache<TPixel>
where
    TPixel: 'static + Send,
{
    ///
    /// Creates an empty pixel program cache
    ///
    pub fn empty() -> PixelProgramCache<TPixel> {
        PixelProgramCache {
            next_program_id:    0,
            phantom_data:       PhantomData,
        }
    }

    ///
    /// Creates a function based on a program that sets its data and scanline data, generating the 'make pixels at position' function
    ///
    fn create_associate_program_data<TProgram>(program: Arc<TProgram>) -> impl Fn(TProgram::ProgramData) -> Box<dyn Fn(i32, &Vec<PixelProgramScanline>) -> Box<dyn Fn(&mut [TPixel], Range<i32>, i32) -> ()>>
    where
        TProgram: 'static + PixelProgram<Pixel=TPixel>,
    {
        move |program_data| {
            // Copy the program
            let program         = Arc::clone(&program);
            let program_data    = Arc::new(program_data);

            // Return a function that takes the scanlines and returns the rendering function
            Box::new(move |min_y, scanlines| {
                let scanline_data   = program.create_scanline_data(min_y, scanlines, &*program_data);
                let program         = Arc::clone(&program);
                let program_data    = Arc::clone(&program_data);

                Box::new(move |target, x_range, y_pos| {
                    program.draw_pixels(target, x_range, y_pos, &*program_data, &scanline_data)
                })
            })
        }
    }

    ///
    /// Caches a pixel program, assigning it an ID, and a cache that can be used
    ///
    pub fn add_program<TProgram>(&mut self, program: TProgram) -> StoredPixelProgram<TProgram> 
    where
        TProgram: 'static + PixelProgram<Pixel=TPixel>,
    {
        // Assign an ID to the new program
        let new_program_id = self.next_program_id;
        let new_program_id = PixelProgramId(new_program_id);

        self.next_program_id += 1;

        // Create the function to associate data with this program
        let associate_data = Box::new(Self::create_associate_program_data(Arc::new(program)));

        // Return a stored pixel program of the appropriate type
        StoredPixelProgram {
            program_id:             new_program_id,
            associate_program_data: associate_data,
        }
    }

    ///
    /// Creates a data cache to store data for rendering a frame with this pixel program cache
    ///
    pub fn create_data_cache(&mut self) -> PixelProgramDataCache<TPixel> {
        PixelProgramDataCache {
            program_data:   vec![],
            scanline_data:  vec![],
        }
    }

    ///
    /// Stores data to be used with an instance of a pixel program
    ///
    /// Program data can be a number of things: in the simplest case it might be the colour that the program 
    ///
    pub fn store_program_data<TProgram>(&mut self, stored_program: &StoredPixelProgram<TProgram>, data_cache: &mut PixelProgramDataCache<TPixel>, data: TProgram::ProgramData) -> PixelProgramDataId 
    where
        TProgram: 'static + PixelProgram<Pixel=TPixel>,
    {
        // Assign an ID to this program data
        let program_data_id = data_cache.program_data.len();

        // Generate the data for this program (well, encapsulate it in a function waiting for the scanline data)
        let associate_scanline_data = (stored_program.associate_program_data)(data);

        // Store in the data cache
        data_cache.program_data.push(associate_scanline_data);

        PixelProgramDataId(program_data_id)
    }

    ///
    /// Creates scanline data for a program 
    ///
    pub fn create_scanline_data(&self, data_cache: &mut PixelProgramDataCache<TPixel>, program_id: PixelProgramId, min_y: i32, scanlines: &Vec<PixelProgramScanline>, program_data: PixelProgramDataId) -> PixelScanlineDataId {
        // The program ID is currently unused as we only need to know the program data ID
        let _program_id = program_id;

        // Assign an ID to this scanline data
        let scanline_data_id = data_cache.scanline_data.len();

        // Generate the scanline data for this program (actually generates the 'run' function)
        let run_program = (data_cache.program_data[program_data.0])(min_y, scanlines);

        // Store in the data cache
        data_cache.scanline_data.push(run_program);

        PixelScanlineDataId(scanline_data_id)
    }

    ///
    /// Runs a program on a range of pixels
    ///
    #[inline]
    pub fn run_program(&self, data_cache: &PixelProgramDataCache<TPixel>, program_id: PixelProgramId, target: &mut [TPixel], x_range: Range<i32>, y_pos: i32, program_data: PixelProgramDataId, scanline_data: PixelScanlineDataId) {
        // The program ID and program data ID are currently unused as we only need to know the scanline ID to run the program
        let _program_id     = program_id;
        let _program_data   = program_data;

        (data_cache.scanline_data[scanline_data.0])(target, x_range, y_pos)
    }
}
