use super::pipeline::*;
use super::render_pass_resources::*;
use super::pipeline_configuration::*;
use crate::buffer::*;

use wgpu;
use wgpu::util;
use wgpu::util::{DeviceExt};

use std::mem;
use std::slice;
use std::sync::*;
use std::ffi::{c_void};

///
/// State for the WGPU renderer
///
pub (crate) struct RendererState {
    /// The device this will render to
    device:                             Arc<wgpu::Device>,

    /// The command queue for the device
    queue:                              Arc<wgpu::Queue>,

    /// The command encoder for this rendering
    pub encoder:                        wgpu::CommandEncoder,

    /// The resources for the next render pass
    pub render_pass_resources:          RenderPassResources,

    /// The pipeline configuration to use with the current rendering
    pub pipeline_configuration:         PipelineConfiguration,

    /// The active pipeline
    pub pipeline:                       Option<Arc<Pipeline>>,

    /// Set to true if the pipeline configuration has changed since it was last committed to the render pass
    pub pipeline_config_changed:        bool,

    /// The pipeline configuration that was last activated
    pub active_pipeline_configuration:  Option<PipelineConfiguration>,

    /// The actions for the active render pass (deferred so we can manage the render pass lifetime)
    pub render_pass:                    Vec<Box<dyn for<'a> FnOnce(&'a RenderPassResources, &mut wgpu::RenderPass<'a>) -> ()>>,

    /// The matrix transform buffer
    pub matrix_buffer:                  Arc<wgpu::Buffer>,
}

impl RendererState {
    ///
    /// Creates a default render state
    ///
    pub fn new(command_queue: Arc<wgpu::Queue>, device: Arc<wgpu::Device>) -> RendererState {
        // TODO: we can avoid re-creating some of these structures every frame: eg, the binding groups in particular

        // Create all the state structures
        let matrix_buffer   = Arc::new(Self::create_transform_buffer(&device));
        let encoder         = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("RendererState::new") });

        RendererState {
            device:                             device,
            queue:                              command_queue,
            encoder:                            encoder,
            render_pass_resources:              RenderPassResources::default(),
            render_pass:                        vec![],
            pipeline_configuration:             PipelineConfiguration::default(),
            pipeline:                           None,
            pipeline_config_changed:            true,
            active_pipeline_configuration:      None,

            matrix_buffer:                      matrix_buffer,
        }
    }

    ///
    /// Updates the contents of the matrix buffer for this renderer
    ///
    #[inline]
    pub fn write_matrix(&mut self, device: &wgpu::Device, new_matrix: &Matrix) {
        let matrix_void     = new_matrix.0.as_ptr() as *const c_void;
        let matrix_len      = mem::size_of::<[[f32; 4]; 4]>();
        let matrix_u8       = unsafe { slice::from_raw_parts(matrix_void as *const u8, matrix_len) };

        let matrix_buffer   = device.create_buffer_init(&util::BufferInitDescriptor {
            label:      Some("matrix_buffer"),
            contents:   matrix_u8,
            usage:      wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        self.matrix_buffer  = Arc::new(matrix_buffer);
    }

    ///
    /// Sets up the transform buffer and layout
    ///
    fn create_transform_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        // Convert the matrix to a u8 pointer
        let matrix          = Matrix::identity();
        let matrix_void     = matrix.0.as_ptr() as *const c_void;
        let matrix_len      = mem::size_of::<[[f32; 4]; 4]>();
        let matrix_u8       = unsafe { slice::from_raw_parts(matrix_void as *const u8, matrix_len) };

        // Load into a buffer
        let matrix_buffer   = device.create_buffer_init(&util::BufferInitDescriptor {
            label:      Some("matrix_buffer"),
            contents:   matrix_u8,
            usage:      wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        matrix_buffer
    }

    ///
    /// Runs the pending render pass
    ///
    pub fn run_render_pass(&mut self) {
        // Take the actions and the resources for this render pass
        let render_actions  = mem::take(&mut self.render_pass);
        let resources       = mem::take(&mut self.render_pass_resources);

        // Keep the current texture view for the next render pass
        self.render_pass_resources.target_view  = resources.target_view.clone();

        // This resets the active pipeline configuration
        self.active_pipeline_configuration      = None;
        self.pipeline_config_changed            = true;

        // Abort early if there are no render actions
        if render_actions.is_empty() {
            return;
        }

        // Start a new render pass using the current encoder
        if let Some(texture_view) = &resources.target_view {
            // Start the render pass
            let mut render_pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:                      Some("run_render_pass"),
                depth_stencil_attachment:   None,
                color_attachments:          &resources.color_attachments(),
            });

            // Run all of the actions
            for action in render_actions.into_iter() {
                (action)(&resources, &mut render_pass);
            }
        }

        // Commit the commands that are pending in the command encoder
        // It's probably not the most efficient way to do things, but it simplifies resource management 
        // a lot (we'll need to hold on to all of the resources from the render pass resources until this
        // is done otherwise). Might be some advantage to committing some commands to the GPU while we
        // generate more too.
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("show_frame_buffer") });
        mem::swap(&mut encoder, &mut self.encoder);

        self.queue.submit(Some(encoder.finish()));
    }
}