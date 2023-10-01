use super::edge_descriptor::*;
use super::edge_intercept::*;
use super::shape_descriptor::*;
use super::shape_id::*;

use smallvec::*;

use flo_sparse_array::*;
use flo_canvas::curves::geo::*;

use std::ops::{Range};

///
/// Data stored for an edge in the edge plan
///
struct EdgeData<TEdge>
where
    TEdge: EdgeDescriptor,
{
    edge: TEdge,
    y_bounds: Range<f64>,
}

///
/// An edge plan describes a 2 dimensional space as a set of edges that divide 
///
pub struct EdgePlan<TEdge>
where
    TEdge: EdgeDescriptor,
{
    /// Describes the shapes
    shapes: SparseArray<ShapeDescriptor>,

    /// The edges themselves
    edges: Vec<EdgeData<TEdge>>,

    /// Where the edges are in space
    edge_space: Space1D<usize>,

    /// The highest edge index that 'prepare_to_render' has been called on
    max_prepared: usize,
}

impl<TEdge> EdgePlan<TEdge>
where
    TEdge: EdgeDescriptor,
{
    ///
    /// Creates an empty edge plan
    ///
    pub fn new() -> EdgePlan<TEdge> {
        EdgePlan {
            shapes:         SparseArray::empty(),
            edges:          vec![],
            edge_space:     Space1D::empty(),
            max_prepared:   0,
        }
    }

    ///
    /// Performs any caching required on the edges so that `intercepts_on_scanlines` will return accurate results
    ///
    #[cfg(feature="multithreading")]
    pub fn prepare_to_render(&mut self) {
        if self.max_prepared != self.edges.len() {
            use rayon::prelude::*;

            // Prepare all of the edges that have not been prepared before
            self.edges.par_iter_mut()
                .skip(self.max_prepared)
                .for_each(|edge| {
                    // Prepare the edge to render
                    edge.edge.prepare_to_render();

                    // The bounding_box() call should have accurate data at this point, so update the edge bounds
                    let ((_, min_y), (_, max_y)) = edge.edge.bounding_box();
                    edge.y_bounds = min_y..max_y;
                });

            // Update the 'max_prepared' value so that we won't prepare edges again
            self.max_prepared = self.edges.len();

            // Update where the edges are in space
            self.edge_space = Space1D::from_data(self.edges.iter()
                .enumerate()
                .map(|(idx, edge)| {
                    (edge.y_bounds.clone(), idx)
                }));
        }
    }

    ///
    /// Performs any caching required on the edges so that `intercepts_on_scanlines` will return accurate results
    ///
    #[cfg(not(feature="multithreading"))]
    pub fn prepare_to_render(&mut self) {
        if self.max_prepared != self.edges.len() {
            // Prepare all of the edges that have not been prepared before
            self.edges.iter_mut()
                .skip(self.max_prepared)
                .for_each(|edge| {
                    // Prepare the edge to render
                    edge.edge.prepare_to_render();

                    // The bounding_box() call should have accurate data at this point, so update the edge bounds
                    let ((_, min_y), (_, max_y)) = edge.edge.bounding_box();
                    edge.y_bounds = min_y..max_y;
                });

            // Update the 'max_prepared' value so that we won't prepare edges again
            self.max_prepared = self.edges.len();

            // Update where the edges are in space
            self.edge_space = Space1D::from_data(self.edges.iter()
                .enumerate()
                .map(|(idx, edge)| {
                    (edge.y_bounds.clone(), idx)
                }));
        }
    }

    ///
    /// Stores the details of how the interior of a shape should be rendered
    ///
    pub fn declare_shape_description(&mut self, shape_id: ShapeId, descriptor: ShapeDescriptor) {
        self.shapes.insert(shape_id.0, descriptor);
    }

    ///
    /// As for `declare_shape_description` but using a 'fluent' API design
    ///
    #[inline]
    pub fn with_shape_description(mut self, shape_id: ShapeId, descriptor: ShapeDescriptor) -> Self {
        (&mut self).declare_shape_description(shape_id, descriptor);
        self
    }

    ///
    /// Returns the z-index for a shape ID
    ///
    #[inline]
    pub fn shape_z_index(&self, shape_id: ShapeId) -> i64 {
        self.shapes.get(shape_id.0).map(|shape| shape.z_index).unwrap_or(0)
    }

    ///
    /// Returns the shape descriptor for a shape ID
    ///
    #[inline]
    pub fn shape_descriptor(&self, shape_id: ShapeId) -> Option<&ShapeDescriptor> {
        self.shapes.get(shape_id.0)
    }

    ///
    /// Adds an edge to this plan
    ///
    #[inline]
    pub fn add_edge(&mut self, new_edge: TEdge) {
        // The y-bounds are calculated later on when we prepare to render
        self.edges.push(EdgeData {
            edge:       new_edge,
            y_bounds:   f64::MIN..f64::MAX,
        });
    }

    ///
    /// As for `add_edge` but using a 'fluent' API design
    ///
    #[inline]
    pub fn with_edge(mut self, new_edge: TEdge) -> Self {
        (&mut self).add_edge(new_edge);
        self
    }

    ///
    /// Declares a shape and all of its edges at once
    ///
    pub fn add_shape(&mut self, shape_id: ShapeId, descriptor: ShapeDescriptor, edges: impl IntoIterator<Item=TEdge>) {
        self.declare_shape_description(shape_id, descriptor);
        for edge in edges {
            self.add_edge(edge);
        }
    }

    ///
    /// As for `add_shape` but using a 'fluent' API design
    ///
    #[inline]
    pub fn with_shape(mut self, shape_id: ShapeId, descriptor: ShapeDescriptor, edges: impl IntoIterator<Item=TEdge>) -> Self {
        (&mut self).add_shape(shape_id, descriptor, edges);
        self
    }

    ///
    /// Returns the edges that intercept a scanline. Shapes are entered on the right-hand side of any intercepts.
    ///
    /// Note that `prepare_to_render()` must have been called before this function can be used to retrieve accurate results.
    ///
    pub fn intercepts_on_scanlines<'a>(&'a self, y_positions: &[f64], output: &mut [Vec<EdgeIntercept>]) {
        // Extend the edge intercepts to cover the number of y-positions we have (can be larger than needed but not smaller)
        let mut edge_intercepts = vec![smallvec![]; y_positions.len()];

        let mut y_min = f64::MAX;
        let mut y_max = f64::MIN;

        y_positions.iter().for_each(|pos| {
            y_min = y_min.min(*pos);
            y_max = y_max.max(*pos);
        });

        // Clear the output
        output.iter_mut().for_each(|val| val.clear());

        // This is the slow way to find the edges that intercept a scanline
        // Possible enhancements
        //  - group up the edges by y position (we can use regions here) so that it's easy to find which edges are on a particular scanline
        //  - pre-sort the edges and only re-sort if there are overlapping edges. Most of the time in an edge region the edges will be intercepted in the
        //      same order
        //  - for anti-aliasing we need a way to track intercepts on the previous scanline for the same shape (usually the same edge, but sometimes the preceding or following edge)
        for edge_idx in self.edge_space.data_in_region(y_min..(y_max+1e-6)) {
            let edge = &self.edges[*edge_idx];

            // Read the intercepts from this edge (we rely on the 'intercepts' method overwriting any old values)
            let shape_id = edge.edge.shape();
            edge.edge.intercepts(y_positions, &mut edge_intercepts);

            for idx in 0..y_positions.len() {
                let output = &mut output[idx];

                for (direction, pos) in edge_intercepts[idx].iter() {
                    output.push(EdgeIntercept { shape: shape_id, direction: *direction, x_pos: *pos });
                }
            }
        }

        // Sort the intercepts on each line by x position
        output.iter_mut().for_each(|intercepts| {
            intercepts.sort_by(|a, b| a.x_pos.total_cmp(&b.x_pos));
        });
    }
}
