use crate::scanplan::*;

///
/// Represents the processed scanline data for a region
///
pub struct ScanlineData {
    /// Describes the plan used to render the scanlines that are input for this program. Index into the array represents the
    /// y position of each scanline.
    scanlines: Vec<ScanlinePlan>,
}
