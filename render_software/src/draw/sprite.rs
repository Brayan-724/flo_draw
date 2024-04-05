use super::canvas_drawing::*;
use super::drawing_state::*;
use super::layer::*;
use super::prepared_layer::*;
use super::texture::*;

use crate::edgeplan::*;
use crate::edges::*;
use crate::filters::*;
use crate::pixel::*;
use crate::pixel_programs::*;

use flo_canvas as canvas;
use smallvec::*;

use std::sync::*;

impl SpriteTransform {
    ///
    /// Returns this transform as a transformation matrix indicating how the points should be transformed
    ///
    #[inline]
    pub (crate) fn matrix(&self) -> canvas::Transform2D {
        match self {
            SpriteTransform::ScaleTransform { scale, translate } =>
                canvas::Transform2D::scale(scale.0 as _, scale.1 as _) * canvas::Transform2D::translate(translate.0 as _, translate.1 as _),

            SpriteTransform::Matrix(matrix) => *matrix
        }
    }
}

impl<TPixel, const N: usize> CanvasDrawing<TPixel, N>
where
    TPixel: 'static + Send + Sync + Pixel<N>,
{
    ///
    /// Selects a sprite for rendering
    ///
    #[inline]
    pub (crate) fn sprite(&mut self, sprite_id: canvas::SpriteId) {
        let transform       = self.current_state.transform;
        let namespace_id    = self.current_namespace;

        // Update the transform of the layer we're leaving
        if let Some(layer) = self.layer(self.current_layer) { layer.last_transform = transform; }

        if let Some(sprite_layer) = self.sprites.get(&(namespace_id, sprite_id)) {
            // Use the existing sprite layer
            self.current_layer = *sprite_layer;
        } else {
            // Create a new sprite layer (sprites are normal layers that aren't rendered until requested)
            let new_layer           = Layer::default();
            let new_layer_handle    = self.next_layer_handle;

            // Advance the next layer handle
            self.next_layer_handle.0 += 1;

            // Add the new layer to the list
            self.layers.insert(new_layer_handle.0, new_layer);

            // Store as a sprite
            self.sprites.insert((self.current_namespace, sprite_id), new_layer_handle);

            // Use the layer we just created
            self.current_layer = new_layer_handle;
        }

        // Update the transform of the layer we're entering
        if let Some(layer) = self.layer(self.current_layer) { layer.last_transform = transform; }
    }

    ///
    /// Moves the content of the specified sprite to the current layer
    ///
    pub (crate) fn sprite_move_from(&mut self, sprite_id: canvas::SpriteId) {
        let namespace_id = self.current_namespace;

        // Clear the current layer to release any resources it's using
        self.clear_layer(self.current_layer);

        if let Some(sprite_layer_handle) = self.sprites.get(&(namespace_id, sprite_id)) {
            // Copy the sprite layer
            let sprite_layer_handle = *sprite_layer_handle;
            let layer_copy          = self.clone_layer(sprite_layer_handle);

            // Replace the current layer with the sprite layer
            self.layers.insert(self.current_layer.0, layer_copy);
        }
    }

    ///
    /// Creates or retrieves the 'prepared' version of the current layer, which can be used to render sprites or textures
    ///
    pub (crate) fn prepare_sprite_layer(&mut self, layer_handle: LayerHandle) -> PreparedLayer {
        if let Some(layer) = self.prepared_layers.get(layer_handle.0) {
            // Use the existing prepared layer
            layer.clone()
        } else if let Some(layer) = self.layers.get(layer_handle.0) {
            // Get the transformation that was used when this layer was last drawn to
            let transform           = layer.last_transform;
            let inverse_transform   = transform.invert().unwrap();

            // Prepare the current layer
            let mut layer = layer.edges.clone();
            layer.prepare_to_render();

            // Calculate the overall bounding box of the layer
            let bounds = layer.bounding_box();

            // Create the prepared layer
            let prepared_layer = PreparedLayer {
                edges:              Arc::new(layer),
                bounds:             bounds,
                transform:          transform,
                inverse_transform:  inverse_transform,
            };

            // Store in the cache (drawing should clear the prepared layer)
            self.prepared_layers.insert(layer_handle.0, prepared_layer.clone());

            prepared_layer
        } else {
            // Layer does not exist
            PreparedLayer {
                edges:              Arc::new(EdgePlan::new()),
                bounds:             ((0.0, 0.0), (0.0, 0.0)),
                transform:          canvas::Transform2D::identity(),
                inverse_transform:  canvas::Transform2D::identity(),
            }
        }
    }

    ///
    /// Returns the filters to add to a combined filter for drawing a sprite
    ///
    pub fn sprite_filter(&mut self, filter: canvas::TextureFilter, width: f64, height: f64) -> Vec<Arc<dyn Send + Sync + PixelFilter<Pixel=TPixel>>> {
        use canvas::TextureFilter::*;

        // TODO: for gaussian blur we can apply both filters at the same time (which is more efficient, but a bit more complicated to implement)
        let filters = match filter {
            GaussianBlur(radius) => {
                let (scale_x, scale_y) = self.sprite_filter_pixel_scale();

                let vertical: Arc<dyn Send + Sync + PixelFilter<Pixel=TPixel>>    = Arc::new(VerticalKernelFilter::with_gaussian_blur_radius(radius as f64 * scale_y));
                let horizontal: Arc<dyn Send + Sync + PixelFilter<Pixel=TPixel>>  = Arc::new(HorizontalKernelFilter::with_gaussian_blur_radius(radius as f64 * scale_x));

                vec![vertical, horizontal]
            }

            AlphaBlend(alpha) => {
                let filter: Arc<dyn Send + Sync + PixelFilter<Pixel=TPixel>> = Arc::new(AlphaBlendFilter::with_alpha(alpha as _));
                vec![filter]
            },

            Mask(mask_texture_id) => {
                let filter: Arc<dyn Send + Sync + PixelFilter<Pixel=TPixel>> = Arc::new(self.sprite_mask_filter(mask_texture_id, width, height));

                vec![filter]
            },

            DisplacementMap(displacement_texture, x_offset, y_offset) => { 
                let filter: Arc<dyn Send + Sync + PixelFilter<Pixel=TPixel>> = Arc::new(self.sprite_displacement_filter(displacement_texture, x_offset as _, y_offset as _, width, height));
                vec![filter]
            },
        };

        filters
    }

    ///
    /// Returns the x and y scale for the current transform setting
    ///
    #[inline]
    fn sprite_filter_pixel_scale(&self) -> (f64, f64) {
        // Figure out the size of a pixel
        let transform   = &self.current_state.transform;

        let (x1, y1)    = transform.transform_point(0.0, 0.0);
        let (x2, y2)    = transform.transform_point(1.0, 1.0);

        let min_x       = f32::min(x1, x2);
        let min_y       = f32::min(y1, y2);
        let max_x       = f32::max(x1, x2);
        let max_y       = f32::max(y1, y2);

        // Size relative to the framebuffer size
        let size_w      = (max_x - min_x)/2.0;
        let size_h      = (max_y - min_y)/2.0;

        (size_w as f64, size_h as f64)
    }

    ///
    /// Creates a mask filter from a texture
    ///
    fn sprite_mask_filter(&mut self, mask_texture_id: canvas::TextureId, width: f64, height: f64) -> MaskFilter<TPixel, N> {
        // Fetch the size of the target texture
        let (texture_width, texture_height) = (width, height);

        // Read the mask texture (we use a 1x1 empty texture if the texture is missing)
        let mask_texture = loop {
            let texture = self.textures.get(&(self.current_namespace, mask_texture_id));
            let texture = if let Some(texture) = texture { texture } else { break Arc::new(U16LinearTexture::from_pixels(1, 1, vec![0, 0, 0, 0])); };

            match &texture.pixels {
                TexturePixels::Empty(_, _) => {
                    break Arc::new(U16LinearTexture::from_pixels(1, 1, vec![0, 0, 0, 0]))
                }

                TexturePixels::Rgba(_) | TexturePixels::Linear(_) => {
                    // Convert to a mip-map so we can read as a U16 texture
                    self.textures.get_mut(&(self.current_namespace, mask_texture_id))
                        .unwrap().make_mip_map(self.gamma);                    
                }

                TexturePixels::MipMap(texture) | TexturePixels::MipMapWithOriginal(_, texture) => {
                    break Arc::clone(texture.mip_level(0));
                }

                TexturePixels::DynamicSprite(dynamic) => {
                    let dynamic = Arc::clone(dynamic);
                    break dynamic.lock().unwrap().get_u16_texture(self);
                }
            }
        };

        let (mask_width, mask_height) = (mask_texture.width(), mask_texture.height());
        let mult_x = mask_width as f64 / texture_width as f64;
        let mult_y = mask_height as f64 / texture_height as f64;

        MaskFilter::with_mask(&mask_texture, mult_x, mult_y)
    }

    ///
    /// Creates a displacement filter from a texture
    ///
    fn sprite_displacement_filter(&mut self, displacement_texture_id: canvas::TextureId, x_offset: f64, y_offset: f64, width: f64, height: f64) -> DisplacementMapFilter<TPixel, N> {
        // Fetch the size of the target texture
        let (texture_width, texture_height) = (width, height);
        let (scale_x, scale_y) = self.sprite_filter_pixel_scale();

        // Read the displacement map texture (we use a 1x1 empty texture if the texture is missing)
        let displacement_texture = loop {
            let texture = self.textures.get(&(self.current_namespace, displacement_texture_id));
            let texture = if let Some(texture) = texture { texture } else { break Arc::new(U16LinearTexture::from_pixels(1, 1, vec![0, 0, 0, 0])); };

            match &texture.pixels {
                TexturePixels::Empty(_, _) => {
                    break Arc::new(U16LinearTexture::from_pixels(1, 1, vec![0, 0, 0, 0]))
                }

                TexturePixels::Rgba(_) | TexturePixels::Linear(_) => {
                    // Convert to a mip-map so we can read as a U16 texture
                    self.textures.get_mut(&(self.current_namespace, displacement_texture_id))
                        .unwrap().make_mip_map(self.gamma);                    
                }

                TexturePixels::MipMap(texture) | TexturePixels::MipMapWithOriginal(_, texture) => {
                    break Arc::clone(texture.mip_level(0));
                }

                TexturePixels::DynamicSprite(dynamic) => {
                    let dynamic = Arc::clone(dynamic);
                    break dynamic.lock().unwrap().get_u16_texture(self);
                }
            }
        };

        let (displ_width, displ_height) = (displacement_texture.width(), displacement_texture.height());
        let mult_x = displ_width as f64 / texture_width as f64;
        let mult_y = displ_height as f64 / texture_height as f64;

        // Create the filter from the texture
        DisplacementMapFilter::with_displacement_map(&displacement_texture, x_offset * scale_x, y_offset * scale_y, mult_x, mult_y, self.gamma)
    }

    ///
    /// Draws the sprite with the specified ID
    ///
    pub (crate) fn sprite_draw_with_filters(&mut self, sprite_id: canvas::SpriteId, filters: Vec<canvas::TextureFilter>) {
        if filters.is_empty() {
            // If there are no filters, then just fall back to the normal sprite draw routine
            self.sprite_draw(sprite_id);
        } else {
            // Otherwise, draw using the filter program
            // TODO: this is very similar to sprite_draw, we shold consolidate these two methods somehow
            use std::iter;

            const VERY_CLOSE: f32 = 1e-12;

            // Get the layer handle for this sprite
            if let Some(sprite_layer_handle) = self.sprites.get(&(self.current_namespace, sprite_id)) {
                // Prepare the sprite layer for rendering
                let sprite_layer = self.prepare_sprite_layer(*sprite_layer_handle);

                if !sprite_layer.edges.is_empty() {
                    // Figure out where the sprite should be rendered on the canvas
                    let ((min_x, min_y), (max_x, max_y)) = sprite_layer.bounds;

                    // Coordinates in terms of render coordinates for the sprite
                    let lower_left  = (min_x as f32, min_y as f32);
                    let lower_right = (max_x as f32, min_y as f32);
                    let upper_left  = (min_x as f32, max_y as f32);
                    let upper_right = (max_x as f32, max_y as f32);

                    // Change to 'origin' coordinates using the inverse transform in the sprite
                    let inverse_transform = sprite_layer.inverse_transform;
                    let lower_left  = inverse_transform.transform_point(lower_left.0, lower_left.1);
                    let lower_right = inverse_transform.transform_point(lower_right.0, lower_right.1);
                    let upper_left  = inverse_transform.transform_point(upper_left.0, upper_left.1);
                    let upper_right = inverse_transform.transform_point(upper_right.0, upper_right.1);

                    // Map back on to the canvas using the sprite transform (generates render coordinates again)
                    let canvas_transform = self.current_state.transform * self.current_state.sprite_transform.matrix();
                    let lower_left  = canvas_transform.transform_point(lower_left.0, lower_left.1);
                    let lower_right = canvas_transform.transform_point(lower_right.0, lower_right.1);
                    let upper_left  = canvas_transform.transform_point(upper_left.0, upper_left.1);
                    let upper_right = canvas_transform.transform_point(upper_right.0, upper_right.1);

                    // Create the filter for this rendering
                    let render_min_x  = lower_left.0.min(upper_left.0).min(lower_right.0).min(upper_right.0);
                    let render_max_x  = lower_left.0.max(upper_left.0).max(lower_right.0).max(upper_right.0);
                    let render_min_y  = lower_left.1.min(upper_left.1).min(lower_right.1).min(upper_right.1);
                    let render_max_y  = lower_left.1.max(upper_left.1).max(lower_right.1).max(upper_right.1);
                    let render_width  = render_max_x - render_min_x;
                    let render_height = render_max_y - render_min_y;

                    let filter: Arc<dyn Send + Sync + PixelFilter<Pixel=TPixel>> = Arc::new(CombinedFilter::from_filters(filters.into_iter()
                        .flat_map(|filter| self.sprite_filter(filter, render_width as _, render_height as _))));

                    // Get the z-index of where to render this sprite
                    let current_layer   = self.layers.get_mut(self.current_layer.0).unwrap();
                    let z_index         = current_layer.z_index;

                    // Future stuff renders on top of the sprite
                    current_layer.z_index += 1;

                    if (lower_left.1-lower_right.1).abs() < VERY_CLOSE && (upper_left.1-upper_right.1).abs() < VERY_CLOSE {
                        let scale_x     = (max_x - min_x) / (lower_right.0 - lower_left.0) as f64;
                        let scale_y     = (max_y - min_y) / (upper_left.1 - lower_left.1) as f64;
                        
                        let translate   = (min_x - (lower_left.0 as f64 * scale_x), min_y - (lower_left.1 as f64 * scale_y));
                        let scale       = (scale_x, scale_y);

                        // Create the brush data
                        let data    = FilteredScanlineData::new(sprite_layer.edges, scale, translate, filter);
                        let data_id = self.program_cache.program_cache.store_program_data(&self.program_cache.filtered_sprite, &mut self.program_data_cache, data);

                        // Shape is a transparent rectangle that runs this program
                        let shape_descriptor = ShapeDescriptor {
                            programs:   smallvec![data_id],
                            is_opaque:  false,
                            z_index:    z_index,
                        };
                        let shape_id = ShapeId::new();

                        // Create a rectangle edge for this data
                        let sprite_edge = RectangleEdge::new(shape_id, (lower_left.0 as f64)..(lower_right.0 as f64), (lower_left.1 as f64)..(upper_left.1 as f64));
                        let sprite_edge: Arc<dyn EdgeDescriptor> = Arc::new(sprite_edge);

                        // Store in the current layer
                        current_layer.edges.add_shape(shape_id, shape_descriptor, iter::once(sprite_edge));
                        current_layer.used_data.push(data_id);
                    } else {
                        // Transform from the coordinates used in the final sprite back to render coordinates
                        let transform           = sprite_layer.inverse_transform * self.current_state.transform;

                        // Map the sprite transform to render coordinates
                        let sprite_transform    = self.current_state.transform * self.current_state.sprite_transform.matrix() * self.current_state.transform.invert().unwrap();

                        // Perform a final transform to generate the transformation from sprite render coordinates to canvas render coordinates
                        let transform           = transform * sprite_transform;

                        // Use the transformed sprite program
                        let edges = sprite_layer.edges.transform(&transform);

                        let data    = FilteredScanlineData::new(Arc::new(edges), (1.0, 1.0), (0.0, 0.0), filter);
                        let data_id = self.program_cache.program_cache.store_program_data(&self.program_cache.filtered_sprite, &mut self.program_data_cache, data);

                        // Shape is a transparent rectangle that runs this program
                        let shape_descriptor = ShapeDescriptor {
                            programs:   smallvec![data_id],
                            is_opaque:  false,
                            z_index:    z_index,
                        };
                        let shape_id = ShapeId::new();

                        // Create a rectangle edge for this data
                        let lower_left  = canvas::Coord2(lower_left.0 as _, lower_left.1 as _);
                        let lower_right = canvas::Coord2(lower_right.0 as _, lower_right.1 as _);
                        let upper_left  = canvas::Coord2(upper_left.0 as _, upper_left.1 as _);
                        let upper_right = canvas::Coord2(upper_right.0 as _, upper_right.1 as _);

                        let sprite_edge = PolylineNonZeroEdge::new(shape_id, vec![lower_left, lower_right, upper_right, upper_left, lower_left]);
                        let sprite_edge: Arc<dyn EdgeDescriptor> = Arc::new(sprite_edge);

                        // Store in the current layer
                        current_layer.edges.add_shape(shape_id, shape_descriptor, iter::once(sprite_edge));
                        current_layer.used_data.push(data_id);
                    }

                    // This 'unprepares' the current layer as for any other drawing operation
                    self.prepared_layers.remove(self.current_layer.0);
                }
            }
        }
    }

    ///
    /// Draws the sprite with the specified ID
    ///
    pub (crate) fn sprite_draw(&mut self, sprite_id: canvas::SpriteId) {
        use std::iter;

        const VERY_CLOSE: f32 = 1e-12;

        // Get the layer handle for this sprite
        if let Some(sprite_layer_handle) = self.sprites.get(&(self.current_namespace, sprite_id)) {
            // Prepare the sprite layer for rendering
            let sprite_layer = self.prepare_sprite_layer(*sprite_layer_handle);

            if !sprite_layer.edges.is_empty() {
                // Figure out where the sprite should be rendered on the canvas
                let ((min_x, min_y), (max_x, max_y)) = sprite_layer.bounds;

                // Coordinates in terms of render coordinates for the sprite
                let lower_left  = (min_x as f32, min_y as f32);
                let lower_right = (max_x as f32, min_y as f32);
                let upper_left  = (min_x as f32, max_y as f32);
                let upper_right = (max_x as f32, max_y as f32);

                // Change to 'origin' coordinates using the inverse transform in the sprite
                let inverse_transform = sprite_layer.inverse_transform;
                let lower_left  = inverse_transform.transform_point(lower_left.0, lower_left.1);
                let lower_right = inverse_transform.transform_point(lower_right.0, lower_right.1);
                let upper_left  = inverse_transform.transform_point(upper_left.0, upper_left.1);
                let upper_right = inverse_transform.transform_point(upper_right.0, upper_right.1);

                // Map back on to the canvas using the sprite transform (generates render coordinates again)
                let canvas_transform = self.current_state.transform * self.current_state.sprite_transform.matrix();
                let lower_left  = canvas_transform.transform_point(lower_left.0, lower_left.1);
                let lower_right = canvas_transform.transform_point(lower_right.0, lower_right.1);
                let upper_left  = canvas_transform.transform_point(upper_left.0, upper_left.1);
                let upper_right = canvas_transform.transform_point(upper_right.0, upper_right.1);

                // Get the z-index of where to render this sprite
                let current_layer   = self.layers.get_mut(self.current_layer.0).unwrap();
                let z_index         = current_layer.z_index;

                // Future stuff renders on top of the sprite
                current_layer.z_index += 1;

                if (lower_left.1-lower_right.1).abs() < VERY_CLOSE && (upper_left.1-upper_right.1).abs() < VERY_CLOSE {
                    let scale_x     = (max_x - min_x) / (lower_right.0 - lower_left.0) as f64;
                    let scale_y     = (max_y - min_y) / (upper_left.1 - lower_left.1) as f64;
                    
                    let translate   = (min_x - (lower_left.0 as f64 * scale_x), min_y - (lower_left.1 as f64 * scale_y));
                    let scale       = (scale_x, scale_y);

                    // Create the brush data
                    let data    = BasicSpriteData::new(sprite_layer.edges, scale, translate);
                    let data_id = self.program_cache.program_cache.store_program_data(&self.program_cache.basic_sprite, &mut self.program_data_cache, data);

                    // Shape is a transparent rectangle that runs this program
                    let shape_descriptor = ShapeDescriptor {
                        programs:   smallvec![data_id],
                        is_opaque:  false,
                        z_index:    z_index,
                    };
                    let shape_id = ShapeId::new();

                    // Create a rectangle edge for this data
                    let sprite_edge = RectangleEdge::new(shape_id, (lower_left.0 as f64)..(lower_right.0 as f64), (lower_left.1 as f64)..(upper_left.1 as f64));
                    let sprite_edge: Arc<dyn EdgeDescriptor> = Arc::new(sprite_edge);

                    // Store in the current layer
                    current_layer.edges.add_shape(shape_id, shape_descriptor, iter::once(sprite_edge));
                    current_layer.used_data.push(data_id);
                } else {
                    // Transform from the coordinates used in the final sprite back to render coordinates
                    let transform           = sprite_layer.inverse_transform * self.current_state.transform;

                    // Map the sprite transform to render coordinates
                    let sprite_transform    = self.current_state.transform * self.current_state.sprite_transform.matrix() * self.current_state.transform.invert().unwrap();

                    // Perform a final transform to generate the transformation from sprite render coordinates to canvas render coordinates
                    let transform           = transform * sprite_transform;

                    // Use the transformed sprite program
                    let data    = TransformedSpriteData::new(sprite_layer.edges, transform);
                    let data_id = self.program_cache.program_cache.store_program_data(&self.program_cache.transformed_sprite, &mut self.program_data_cache, data);

                    // Shape is a polyline for the bounds of the sprite
                    let shape_descriptor = ShapeDescriptor {
                        programs:   smallvec![data_id],
                        is_opaque:  false,
                        z_index:    z_index,
                    };
                    let shape_id = ShapeId::new();

                    // Create a rectangle edge for this data
                    let lower_left  = canvas::Coord2(lower_left.0 as _, lower_left.1 as _);
                    let lower_right = canvas::Coord2(lower_right.0 as _, lower_right.1 as _);
                    let upper_left  = canvas::Coord2(upper_left.0 as _, upper_left.1 as _);
                    let upper_right = canvas::Coord2(upper_right.0 as _, upper_right.1 as _);

                    let sprite_edge = PolylineNonZeroEdge::new(shape_id, vec![lower_left, lower_right, upper_right, upper_left, lower_left]);
                    let sprite_edge: Arc<dyn EdgeDescriptor> = Arc::new(sprite_edge);

                    // Store in the current layer
                    current_layer.edges.add_shape(shape_id, shape_descriptor, iter::once(sprite_edge));
                    current_layer.used_data.push(data_id);
                }

                // This 'unprepares' the current layer as for any other drawing operation
                self.prepared_layers.remove(self.current_layer.0);
            }
        }
    }
}

impl DrawingState {
    ///
    /// Applies a canvas sprite transform to the current drawing state
    ///
    pub (crate) fn sprite_transform(&mut self, transform: canvas::SpriteTransform) {
        use canvas::SpriteTransform::*;

        let sprite_transform = &mut self.sprite_transform;

        match (transform, sprite_transform) {
            (Identity, transform)                                                   => *transform = SpriteTransform::ScaleTransform { scale: (1.0, 1.0), translate: (0.0, 0.0) },

            (Translate(x, y), SpriteTransform::ScaleTransform { translate, scale }) => { translate.0 += x as f64 * scale.0; translate.1 += y as f64 * scale.0; }
            (Scale(x, y), SpriteTransform::ScaleTransform { scale, .. })            => { scale.0 *= x as f64; scale.1 *= y as f64; }

            (Rotate(theta), sprite_transform)                                       => { *sprite_transform = SpriteTransform::Matrix(sprite_transform.matrix() * canvas::Transform2D::rotate_degrees(theta)); }
            (Transform2D(matrix), sprite_transform)                                 => { *sprite_transform = SpriteTransform::Matrix(sprite_transform.matrix() * matrix); }
        
            (Translate(x, y), SpriteTransform::Matrix(t))                           => { *t = *t * canvas::Transform2D::translate(x, y); }
            (Scale(x, y), SpriteTransform::Matrix(t))                               => { *t = *t * canvas::Transform2D::scale(x, y); }
        }
    }
}