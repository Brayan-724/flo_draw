use super::wgpu_shader::*;
use super::shader_cache::*;
use crate::action::*;
use crate::buffer::*;

use wgpu;

use std::mem;

///
/// Description of a WGPU pipeline configuration (used to create the configuration and as a hash key)
///
#[derive(Clone, PartialEq, Eq, Hash)]
pub (crate) struct PipelineConfiguration {
    /// Format of the texture that this will render against
    pub (crate) texture_format:         wgpu::TextureFormat,

    /// The identifier of the shader module to use (this defines both the vertex and the fragment shader, as well as the pipeline layout to use)
    pub (crate) shader_module:          WgpuShader,

    /// The blending mode for this pipeline configuration
    pub (crate) blending_mode:          BlendMode,

    /// The number of samples the target texture uses (or None for no multisampling)
    pub (crate) multisampling_count:    Option<u32>,
}

impl Default for PipelineConfiguration {
    fn default() -> PipelineConfiguration {
        PipelineConfiguration {
            texture_format:         wgpu::TextureFormat::Bgra8Unorm,
            shader_module:          WgpuShader::default(),
            blending_mode:          BlendMode::SourceOver,
            multisampling_count:    None
        }
    }
}

#[inline]
fn create_add_blend_state(rgb_src_factor: wgpu::BlendFactor, rgb_dst_factor: wgpu::BlendFactor, alpha_src_factor: wgpu::BlendFactor, alpha_dst_factor: wgpu::BlendFactor) -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: rgb_src_factor,
            dst_factor: rgb_dst_factor,
            operation:  wgpu::BlendOperation::Add,
        },

        alpha: wgpu::BlendComponent {
            src_factor: alpha_src_factor,
            dst_factor: alpha_dst_factor,
            operation:  wgpu::BlendOperation::Add,
        }
    }
}

#[inline]
fn create_op_blend_state(rgb_src_factor: wgpu::BlendFactor, rgb_dst_factor: wgpu::BlendFactor, alpha_src_factor: wgpu::BlendFactor, alpha_dst_factor: wgpu::BlendFactor, color_op: wgpu::BlendOperation, alpha_op: wgpu::BlendOperation) -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: rgb_src_factor,
            dst_factor: rgb_dst_factor,
            operation:  color_op,
        },

        alpha: wgpu::BlendComponent {
            src_factor: alpha_src_factor,
            dst_factor: alpha_dst_factor,
            operation:  alpha_op,
        }
    }
}

///
/// Annoyingly, the pipeline descriptor borrows its data structures, so we need some temp storage for it to borrow from
/// (it's not possible to separate creating the descriptor from the pipeline itself without this structure due to this
/// aspect design of wgpu)
///
pub struct PipelineDescriptorTempStorage {
    color_targets:      Vec<Option<wgpu::ColorTargetState>>,
}

impl Default for PipelineDescriptorTempStorage {
    fn default() -> PipelineDescriptorTempStorage {
        PipelineDescriptorTempStorage {
            color_targets: vec![]
        }
    }
}

impl PipelineConfiguration {
    ///
    /// Retrieves the configured blend state for this pipeline
    ///
    #[inline]
    pub fn blend_state(&self) -> Option<wgpu::BlendState> {
        use self::BlendMode::*;
        use wgpu::BlendFactor::*;
        use wgpu::BlendOperation::*;

        match self.blending_mode {
            SourceOver          => Some(create_add_blend_state(SrcAlpha, OneMinusSrcAlpha, One, OneMinusSrcAlpha)),
            DestinationOver     => Some(create_add_blend_state(OneMinusDstAlpha, DstAlpha, OneMinusDstAlpha, One)),
            SourceIn            => Some(create_add_blend_state(DstAlpha, Zero, DstAlpha, Zero)),
            DestinationIn       => Some(create_add_blend_state(Zero, SrcAlpha, Zero, SrcAlpha)),
            SourceOut           => Some(create_add_blend_state(Zero, OneMinusDstAlpha, Zero, OneMinusDstAlpha)),
            DestinationOut      => Some(create_add_blend_state(Zero, OneMinusSrcAlpha, Zero, OneMinusSrcAlpha)),
            SourceATop          => Some(create_add_blend_state(OneMinusDstAlpha, SrcAlpha, OneMinusDstAlpha, SrcAlpha)),
            DestinationATop     => Some(create_add_blend_state(OneMinusDstAlpha, OneMinusSrcAlpha, OneMinusDstAlpha, OneMinusSrcAlpha)),

            // Multiply is a*b. Here we multiply the source colour by the destination colour, then blend the destination back in again to take account of
            // alpha in the source layer (this version of multiply has no effect on the target alpha value: a more strict version might multiply those too)
            //
            // The source side is precalculated so that an alpha of 0 produces a colour of 1,1,1 to take account of transparency in the source.
            Multiply            => Some(create_add_blend_state(Dst, Zero, Zero, One)),

            // TODO: screen is 1-(1-a)*(1-b) which I think is harder to fake. If we precalculate (1-a) as the src in the shader
            // then can multiply by ONE_MINUS_DST_COLOR to get (1-a)*(1-b). Can use One as our target colour, and then a 
            // reverse subtraction to get 1-(1-a)*(1-b)
            // (This implementation doesn't work: the One is 1*DST_COLOR and not 1 so this is currently 1*b-(1-a)*(1-b)
            // with shader support)
            Screen              => Some(create_op_blend_state(OneMinusDst, One, Zero, One, ReverseSubtract, Add)),

            AllChannelAlphaSourceOver       => Some(create_add_blend_state(One, OneMinusDst, One, OneMinusSrcAlpha)),
            AllChannelAlphaDestinationOver  => Some(create_add_blend_state(OneMinusDst, One, OneMinusDstAlpha, One)),
        }
    }

    ///
    /// Creates the colour target states for this pipeline
    ///
    #[inline]
    pub fn color_targets(&self) -> Vec<Option<wgpu::ColorTargetState>> {
        let blend_state = self.blend_state();

        vec![
            Some(wgpu::ColorTargetState {
                format:     self.texture_format,
                blend:      blend_state,
                write_mask: wgpu::ColorWrites::ALL, 
            })
        ]
    }

    ///
    /// Returns the vertex buffer layout we'll use for this pipeline configuration
    ///
    fn vertex_buffer_layout(&self) -> &[wgpu::VertexBufferLayout] {
        let layout: &'static [wgpu::VertexBufferLayout] = &[wgpu::VertexBufferLayout {
            array_stride:   mem::size_of::<Vertex2D>() as _,
            step_mode:      wgpu::VertexStepMode::Vertex,
            attributes:     &[
                wgpu::VertexAttribute {
                    // pos
                    format:             wgpu::VertexFormat::Float32x2,
                    offset:             0, 
                    shader_location:    0,
                },

                wgpu::VertexAttribute {
                    // tex_coord
                    format:             wgpu::VertexFormat::Float32x2,
                    offset:             (mem::size_of::<f32>()*2) as _,
                    shader_location:    1,
                },

                wgpu::VertexAttribute {
                    // color
                    format:             wgpu::VertexFormat::Uint8x4,
                    offset:             (mem::size_of::<f32>()*2 + mem::size_of::<f32>()*2) as _,
                    shader_location:    2,
                },
            ]
        }];

        layout
    }

    ///
    /// Creates the vertex state for this pipeline
    ///
    #[inline]
    fn vertex_state<'a>(&'a self, shader_cache: &'a ShaderCache<WgpuShader>) -> wgpu::VertexState<'a> {
        // Fetch the shader module
        let (shader_module, vertex_fn, _) = shader_cache.get_shader(&self.shader_module).unwrap();

        wgpu::VertexState {
            module:         shader_module,
            entry_point:    vertex_fn,
            buffers:        self.vertex_buffer_layout(),
        }
    }

    ///
    /// Creates the fragment state for this render pipeline. The temp storage must be initialised with the color targets prior to this call
    ///
    #[inline]
    fn fragment_state<'a>(&'a self, shader_cache: &'a ShaderCache<WgpuShader>, temp_storage: &'a PipelineDescriptorTempStorage) -> Option<wgpu::FragmentState<'a>> {
        // Fetch the shader module
        let (shader_module, _, fragment_fn) = shader_cache.get_shader(&self.shader_module).unwrap();

        Some(wgpu::FragmentState {
            module:         shader_module,
            entry_point:    fragment_fn,
            targets:        &temp_storage.color_targets,
        })
    }

    ///
    /// Creates the matrix bind group layout descriptor for this configuration (this is bind group 0 in the shaders)
    ///
    #[inline]
    pub fn matrix_bind_group_layout<'a>(&'a self) -> wgpu::BindGroupLayoutDescriptor<'a> {
        // Rust doesn't seem to be able to do the same trick with &'static here as we do in vertex_buffer_layout so we declare an actual
        // static here to achieve the same thing (part of the annoying 'complicated structure borrows things recursively' dance wgpu 
        // makes us do)
        static JUST_MATRIX: [wgpu::BindGroupLayoutEntry; 1] = [
            // Matrix
            wgpu::BindGroupLayoutEntry {
                binding:            0,
                visibility:         wgpu::ShaderStages::VERTEX,
                count:              None,
                ty:                 wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   wgpu::BufferSize::new(64),
                }
            },
        ];

        wgpu::BindGroupLayoutDescriptor {
            label:      Some("matrix_bind_group_layout"),
            entries:    &JUST_MATRIX,
        }
    }

    ///
    /// Creates the bind group layout for the clipping mask bind group (this is bind group 1 in the shaders)
    ///
    #[inline]
    pub fn clip_mask_bind_group_layout<'a>(&'a self) -> wgpu::BindGroupLayoutDescriptor<'a> {
        // There are two types of binding layout: with and without the clip mask texture
        static NO_CLIP_MASK:    [wgpu::BindGroupLayoutEntry; 0] = [];
        static WITH_CLIP_MASK:  [wgpu::BindGroupLayoutEntry; 1] = [
            wgpu::BindGroupLayoutEntry {
                binding:            0,
                visibility:         wgpu::ShaderStages::FRAGMENT,
                count:              None,
                ty:                 wgpu::BindingType::Texture {
                    sample_type:    wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled:   true,
                }
            },
        ];

        // The type of binding that's in use depends on if the shader module has a clipping mask or not
        match self.shader_module {
            WgpuShader::Texture(StandardShaderVariant::ClippingMask, _, _, _)   |
            WgpuShader::Simple(StandardShaderVariant::ClippingMask, _)          => {
                wgpu::BindGroupLayoutDescriptor {
                    label:      Some("clip_mask_bind_group_layout_with_clip_mask"),
                    entries:    &WITH_CLIP_MASK,
                }
            }

            WgpuShader::Texture(StandardShaderVariant::NoClipping, _, _, _)     |
            WgpuShader::Simple(StandardShaderVariant::NoClipping, _)            => {
                wgpu::BindGroupLayoutDescriptor {
                    label:      Some("clip_mask_bind_group_layout_no_clip_mask"),
                    entries:    &NO_CLIP_MASK,
                }
            }
        }
    }

    ///
    /// Creates the bind group layout descriptor for the texture bind group (this is bind group 2 in the shaders)
    ///
    #[inline]
    pub fn texture_bind_group_layout<'a>(&'a self) -> wgpu::BindGroupLayoutDescriptor<'a> {
        static NO_TEXTURE: [wgpu::BindGroupLayoutEntry; 0]          = [];
        static WITH_SAMPLER: [wgpu::BindGroupLayoutEntry; 3]        = [
            wgpu::BindGroupLayoutEntry {
                binding:            0,
                visibility:         wgpu::ShaderStages::FRAGMENT,
                count:              None,
                ty:                 wgpu::BindingType::Texture {
                    sample_type:    wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled:   false,
                }
            },
            wgpu::BindGroupLayoutEntry {
                binding:            1,
                visibility:         wgpu::ShaderStages::FRAGMENT,
                count:              None,
                ty:                 wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            },
            wgpu::BindGroupLayoutEntry {
                binding:            2,
                visibility:         wgpu::ShaderStages::FRAGMENT,
                count:              None,
                ty:                 wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   wgpu::BufferSize::new(4),
                }
            },
        ];
        static WITH_MULTISAMPLE: [wgpu::BindGroupLayoutEntry; 2]    = [
            wgpu::BindGroupLayoutEntry {
                binding:            0,
                visibility:         wgpu::ShaderStages::FRAGMENT,
                count:              None,
                ty:                 wgpu::BindingType::Texture {
                    sample_type:    wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled:   false,
                }
            },
            wgpu::BindGroupLayoutEntry {
                binding:            1,
                visibility:         wgpu::ShaderStages::FRAGMENT,
                count:              None,
                ty:                 wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   wgpu::BufferSize::new(4),
                }
            },
        ];

        match self.shader_module {
            WgpuShader::Texture(_, InputTextureType::Sampler, _, _) => {
                wgpu::BindGroupLayoutDescriptor {
                    label:      Some("texture_bind_group_layout_sampler"),
                    entries:    &WITH_SAMPLER,
                }
            },

            WgpuShader::Texture(_, InputTextureType::Multisampled, _, _) => {
                wgpu::BindGroupLayoutDescriptor {
                    label:      Some("texture_bind_group_layout_multisampled"),
                    entries:    &WITH_MULTISAMPLE,
                }
            },

            WgpuShader::Texture(_, InputTextureType::None, _, _) => {
                wgpu::BindGroupLayoutDescriptor {
                    label:      Some("texture_bind_group_layout_none"),
                    entries:    &NO_TEXTURE,
                }
            },

            WgpuShader::Simple(_, _) => {
                wgpu::BindGroupLayoutDescriptor {
                    label:      Some("texture_bind_group_layout_none"),
                    entries:    &NO_TEXTURE,
                }
            }
        }
    }

    ///
    /// Creates the pipeline layout for this render pipeline
    ///
    #[inline]
    pub fn pipeline_layout<'a>(&self, bind_group_layouts: &'a [&'a wgpu::BindGroupLayout]) -> wgpu::PipelineLayoutDescriptor<'a> {
        wgpu::PipelineLayoutDescriptor {
            label:                  Some("pipeline_layout"),
            bind_group_layouts:     bind_group_layouts,
            push_constant_ranges:   &[],
        }
    }

    ///
    /// Creates the render pipeline descriptor for this render pipeline
    ///
    #[inline]
    pub fn render_pipeline_descriptor<'a>(&'a self, shader_cache: &'a mut ShaderCache<WgpuShader>, pipeline_layout: &'a wgpu::PipelineLayout, temp_storage: &'a mut PipelineDescriptorTempStorage) -> wgpu::RenderPipelineDescriptor<'a> {
        // Fill up the temp storage
        temp_storage.color_targets = self.color_targets();

        // Load the shaders so that vertex_state and fragment_state can find them
        shader_cache.load_shader(&self.shader_module);

        // Decide on the multisampling state
        let multisampling = if let Some(sample_count) = self.multisampling_count {
            wgpu::MultisampleState {
                count:                      sample_count,
                mask:                       !0,
                alpha_to_coverage_enabled:  true,
            }
        } else {
            wgpu::MultisampleState::default()
        };

        wgpu::RenderPipelineDescriptor {
            label:          Some("render_pipeline_descriptor"),
            layout:         Some(pipeline_layout),
            vertex:         self.vertex_state(shader_cache),
            fragment:       self.fragment_state(shader_cache, temp_storage),
            primitive:      wgpu::PrimitiveState::default(),
            depth_stencil:  None,
            multisample:    multisampling,
            multiview:      None,
        }
    }
}
