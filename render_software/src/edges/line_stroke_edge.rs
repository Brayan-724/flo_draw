use super::bezier_subpath_edge::*;
use super::flattened_bezier_subpath_edge::*;
use crate::edgeplan::*;

use flo_canvas as canvas;
use flo_canvas::curves::bezier::path::*;
use flo_canvas::curves::geo::*;
use flo_canvas::curves::bezier::*;

use smallvec::*;
use itertools::*;

use std::iter;
use std::sync::*;
use std::vec;

///
/// The edges generated by creating a thick line stroke from a path
///
#[derive(Clone)]
pub struct LineStrokeEdge {
    /// The shape ID of this edge
    shape_id: ShapeId,

    /// The options to use for generating the stroke
    stroke_options: StrokeOptions,

    /// The width of the line that this should generate
    width: f64,

    /// The edges of the current path in this drawing state
    path_edges: Vec<Curve<Coord2>>,

    /// Indexes of the points where the subpaths starts
    subpaths: Vec<usize>,

    /// After being prepared: the bezier path for the line stroke
    bezier_path: Vec<BezierSubpathNonZeroEdge>,
}

impl LineStrokeEdge {
    ///
    /// Creates a new line stroke edge
    ///
    /// Subpaths are the indexes into the `path_edges` list that indicate where the stroke should be divided
    ///
    #[inline]
    pub fn new(shape_id: ShapeId, path_edges: Vec<Curve<Coord2>>, subpaths: Vec<usize>, width: f64, stroke_options: StrokeOptions) -> Self {
        LineStrokeEdge {
            shape_id:       shape_id,
            stroke_options: stroke_options,
            width:          width,
            path_edges:     path_edges,
            subpaths:       subpaths,
            bezier_path:    vec![],
        }
    }
}

#[inline]
fn transform_coord(point: &canvas::Coord2, transform: &canvas::Transform2D) -> canvas::Coord2 {
    let (x, y) = transform.transform_point(point.x() as _, point.y() as _);

    Coord2(x as _, y as _)
}

impl EdgeDescriptor for LineStrokeEdge {
    fn clone_as_object(&self) -> Arc<dyn EdgeDescriptor> {
        Arc::new(self.clone())
    }

    fn prepare_to_render(&mut self) {
        self.bezier_path.clear();

        // Create bezier subpaths
        for (start_idx, end_idx) in self.subpaths.iter().copied().chain(iter::once(self.path_edges.len())).tuple_windows() {
            if start_idx >= end_idx { continue; }

            // Use a path builder to create a simple bezier path
            let mut path = BezierPathBuilder::<SimpleBezierPath>::start(self.path_edges[start_idx].start_point());
            for curve in self.path_edges[start_idx..end_idx].iter() {
                path = path.curve_to(curve.control_points(), curve.end_point());
            }

            let path = path.build();

            // Thicken it using the path stroking algorithm
            let stroked_path = stroke_path::<BezierSubpath, _>(&path, self.width, &self.stroke_options);

            // Render this path using the non-zero winding rule
            for subpath in stroked_path.into_iter() {
                self.bezier_path.push(subpath.to_non_zero_edge(ShapeId(0)));
            }
        }

        // Prepare the paths we created for rendering
        for path in self.bezier_path.iter_mut() {
            path.prepare_to_render();
        }
    }

    fn transform(&self, transform: &canvas::Transform2D) -> Arc<dyn EdgeDescriptor> {
        // Convert the edges
        let path_edges = self.path_edges.iter()
            .map(|curve| {
                let (sp, (cp1, cp2), ep) = curve.all_points();
                let sp  = transform_coord(&sp, &transform);
                let cp1 = transform_coord(&cp1, &transform);
                let cp2 = transform_coord(&cp2, &transform);
                let ep  = transform_coord(&ep, &transform);

                Curve::from_points(sp, (cp1, cp2), ep)
            })
            .collect();

        if self.bezier_path.len() != 0 {
            // This edge was already prepared, so transform the bezier path
            let bezier_path = self.bezier_path.iter()
                .map(|bezier_path| {
                    let mut path = bezier_path.transform_as_self(transform);
                    path.prepare_to_render();
                    path
                })
                .collect();

            Arc::new(LineStrokeEdge {
                shape_id:       self.shape_id,
                stroke_options: self.stroke_options,
                width:          self.width,
                path_edges:     path_edges,
                subpaths:       self.subpaths.clone(),
                bezier_path:    bezier_path,
            })
        } else {
            // Edge was not prepared: create with no bezier path, then prepare it
            let mut new_edge = LineStrokeEdge {
                shape_id:       self.shape_id,
                stroke_options: self.stroke_options,
                width:          self.width,
                path_edges:     path_edges,
                subpaths:       self.subpaths.clone(),
                bezier_path:    vec![],
            };
            new_edge.prepare_to_render();

            Arc::new(new_edge)
        }
    }

    fn shape(&self) -> ShapeId {
        self.shape_id
    }

    fn bounding_box(&self) -> ((f64, f64), (f64, f64)) {
        let (mut min_x, mut min_y)  = (f64::MAX, f64::MAX);
        let (mut max_x, mut max_y)  = (f64::MIN, f64::MIN);

        for path in self.bezier_path.iter() {
            let ((path_min_x, path_min_y), (path_max_x, path_max_y)) = path.bounding_box();

            min_x = min_x.min(path_min_x);
            min_y = min_y.min(path_min_y);
            max_x = max_x.max(path_max_x);
            max_y = max_y.max(path_max_y);
        }

        ((min_x, min_y), (max_x, max_y))
    }

    fn intercepts(&self, y_positions: &[f64], output: &mut [SmallVec<[EdgeDescriptorIntercept; 2]>]) {
        match self.bezier_path.len() {
            0 => { }
            1 => { self.bezier_path[0].intercepts(y_positions, output) }

            _ => {
                // Fill the initial set of inputs
                self.bezier_path[0].intercepts(y_positions, output);

                // Also add in the intercepts from the other paths
                let mut tmp_output = vec![smallvec![]; y_positions.len()];

                for path in self.bezier_path.iter().skip(1) {
                    // Get the intercepts for this path
                    path.intercepts(y_positions, &mut tmp_output);

                    // Append to the result
                    for (tmp, output) in tmp_output.iter_mut().zip(output.iter_mut()) {
                        output.extend(tmp.drain(..))
                    }
                }

                // Result must be sorted
                for output in output.iter_mut() {
                    output.sort_by(|a, b| a.x_pos.total_cmp(&b.x_pos));
                }
            }
        }
    }
}

///
/// The edges generated by creating a thick line stroke from a path, which will be rendered by flattening the bezier path to a polyline
///
#[derive(Clone)]
pub struct FlattenedLineStrokeEdge {
    /// The shape ID of this edge
    shape_id: ShapeId,

    /// The options to use for generating the stroke
    stroke_options: StrokeOptions,

    /// The width of the line that this should generate
    width: f64,

    /// The edges of the current path in this drawing state
    path_edges: Vec<Curve<Coord2>>,

    /// Indexes of the points where the subpaths starts
    subpaths: Vec<usize>,

    /// After being prepared: the bezier path for the line stroke
    bezier_path: Vec<FlattenedBezierNonZeroEdge>,
}

impl FlattenedLineStrokeEdge {
    ///
    /// Creates a new line stroke edge
    ///
    /// Subpaths are the indexes into the `path_edges` list that indicate where the stroke should be divided
    ///
    #[inline]
    pub fn new(shape_id: ShapeId, path_edges: Vec<Curve<Coord2>>, subpaths: Vec<usize>, width: f64, stroke_options: StrokeOptions) -> Self {
        FlattenedLineStrokeEdge {
            shape_id:       shape_id,
            stroke_options: stroke_options,
            width:          width,
            path_edges:     path_edges,
            subpaths:       subpaths,
            bezier_path:    vec![],
        }
    }
}

impl EdgeDescriptor for FlattenedLineStrokeEdge {
    fn clone_as_object(&self) -> Arc<dyn EdgeDescriptor> {
        Arc::new(self.clone())
    }

    fn prepare_to_render(&mut self) {
        self.bezier_path.clear();

        // Create bezier subpaths
        for (start_idx, end_idx) in self.subpaths.iter().copied().chain(iter::once(self.path_edges.len())).tuple_windows() {
            if start_idx >= end_idx { continue; }

            // Use a path builder to create a simple bezier path
            let mut path = BezierPathBuilder::<SimpleBezierPath>::start(self.path_edges[start_idx].start_point());
            for curve in self.path_edges[start_idx..end_idx].iter() {
                path = path.curve_to(curve.control_points(), curve.end_point());
            }

            let path = path.build();

            // Thicken it using the path stroking algorithm
            let stroked_path = stroke_path::<BezierSubpath, _>(&path, self.width, &self.stroke_options);

            // Render this path using the non-zero winding rule
            for subpath in stroked_path.into_iter() {
                self.bezier_path.push(subpath.to_flattened_non_zero_edge(ShapeId(0)));
            }
        }

        // Prepare the paths we created for rendering
        for path in self.bezier_path.iter_mut() {
            path.prepare_to_render();
        }
    }

    fn shape(&self) -> ShapeId {
        self.shape_id
    }

    fn bounding_box(&self) -> ((f64, f64), (f64, f64)) {
        let (mut min_x, mut min_y)  = (f64::MAX, f64::MAX);
        let (mut max_x, mut max_y)  = (f64::MIN, f64::MIN);

        for path in self.bezier_path.iter() {
            let ((path_min_x, path_min_y), (path_max_x, path_max_y)) = path.bounding_box();

            min_x = min_x.min(path_min_x);
            min_y = min_y.min(path_min_y);
            max_x = max_x.max(path_max_x);
            max_y = max_y.max(path_max_y);
        }

        ((min_x, min_y), (max_x, max_y))
    }

    fn transform(&self, transform: &canvas::Transform2D) -> Arc<dyn EdgeDescriptor> {
        // Convert the edges
        let path_edges = self.path_edges.iter()
            .map(|curve| {
                let (sp, (cp1, cp2), ep) = curve.all_points();
                let sp  = transform_coord(&sp, &transform);
                let cp1 = transform_coord(&cp1, &transform);
                let cp2 = transform_coord(&cp2, &transform);
                let ep  = transform_coord(&ep, &transform);

                Curve::from_points(sp, (cp1, cp2), ep)
            })
            .collect();

        if self.bezier_path.len() != 0 {
            // This edge was already prepared, so transform the bezier path
            let bezier_path = self.bezier_path.iter()
                .map(|bezier_path| bezier_path.transform_as_self(transform))
                .collect::<Vec<_>>();

            Arc::new(FlattenedLineStrokeEdge {
                shape_id:       self.shape_id,
                stroke_options: self.stroke_options,
                width:          self.width,
                path_edges:     path_edges,
                subpaths:       self.subpaths.clone(),
                bezier_path:    bezier_path,
            })
        } else {
            // Edge was not prepared: create with no bezier path, then prepare it
            let mut new_edge = LineStrokeEdge {
                shape_id:       self.shape_id,
                stroke_options: self.stroke_options,
                width:          self.width,
                path_edges:     path_edges,
                subpaths:       self.subpaths.clone(),
                bezier_path:    vec![],
            };
            new_edge.prepare_to_render();

            Arc::new(new_edge)
        }
    }

    fn intercepts(&self, y_positions: &[f64], output: &mut [SmallVec<[EdgeDescriptorIntercept; 2]>]) {
        match self.bezier_path.len() {
            0 => { }
            1 => { self.bezier_path[0].intercepts(y_positions, output) }

            _ => {
                // Fill the initial set of inputs
                self.bezier_path[0].intercepts(y_positions, output);

                // Also add in the intercepts from the other paths
                let mut tmp_output = vec![smallvec![]; y_positions.len()];

                for path in self.bezier_path.iter().skip(1) {
                    // Get the intercepts for this path
                    path.intercepts(y_positions, &mut tmp_output);

                    // Append to the result
                    for (tmp, output) in tmp_output.iter_mut().zip(output.iter_mut()) {
                        output.extend(tmp.drain(..))
                    }
                }

                // Result must be sorted
                for output in output.iter_mut() {
                    output.sort_by(|a, b| a.x_pos.total_cmp(&b.x_pos));
                }
            }
        }
    }
}
