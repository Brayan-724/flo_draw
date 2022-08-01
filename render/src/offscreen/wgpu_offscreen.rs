use super::error::*;
use super::offscreen_trait::*;

use crate::action::*;
use crate::wgpu_renderer::*;

use ::desync::*;

use wgpu;
use futures::prelude::*;

use std::sync::*;

lazy_static! {
    static ref WGPU_BACKGROUND: Desync<()> = Desync::new(());
}

///
/// A WGPU offscreen render context
///
struct WgpuOffscreenRenderContext {
    instance:   Arc<wgpu::Instance>,
    device:     Arc<wgpu::Device>,
    adapter:    Arc<wgpu::Adapter>,
    queue:      Arc<wgpu::Queue>,
}

struct WgpuOffscreenRenderTarget {
    texture:    Arc<wgpu::Texture>,
    device:     Arc<wgpu::Device>,
    queue:      Arc<wgpu::Queue>,
    renderer:   WgpuRenderer,
    size:       (u32, u32),
}

///
/// Performs on-startup initialisation steps for offscreen rendering using the WGPU implementation
///
/// Only required if not using a toolkit renderer (eg, in an HTTP renderer or command-line tool). Will likely replace
/// the bindings for any GUI toolkit, so this is not appropriate for desktop-type apps.
///
/// This version is the Metal version for Mac OS X
///
pub async fn wgpu_initialize_offscreen_rendering() -> Result<impl OffscreenRenderContext, RenderInitError> {
    // Create a new WGPU instance and adapter
    let instance    = wgpu::Instance::new(wgpu::Backends::all());
    let adapter     = instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference:       wgpu::PowerPreference::default(),
        force_fallback_adapter: false,
        compatible_surface:     None,
    }).await.unwrap();

    // Fetch the device and the queue
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            label:      None,
            features:   wgpu::Features::empty(),
            limits:     wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
        }, None).await.unwrap();

    // Result is a WGPU offscreen render context
    Ok(WgpuOffscreenRenderContext {
        instance:   Arc::new(instance),
        device:     Arc::new(device),
        adapter:    Arc::new(adapter),
        queue:      Arc::new(queue),
    })
}

///
/// Performs on-startup initialisation steps for offscreen rendering
///
/// Only required if not using a toolkit renderer (eg, in an HTTP renderer or command-line tool). Will likely replace
/// the bindings for any GUI toolkit, so this is not appropriate for desktop-type apps.
///
/// This version is the Metal version for Mac OS X
///
pub fn initialize_offscreen_rendering() -> Result<impl OffscreenRenderContext, RenderInitError> {
    WGPU_BACKGROUND.future_desync(|_| async { wgpu_initialize_offscreen_rendering().await }.boxed()).sync().unwrap()
}

impl OffscreenRenderContext for WgpuOffscreenRenderContext {
    type RenderTarget = WgpuOffscreenRenderTarget;

    ///
    /// Creates a new render target for this context
    ///
    fn create_render_target(&mut self, width: usize, height: usize) -> Self::RenderTarget {
        // Create a texture to render on
        let target_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label:              Some("WgpuOffscreenRenderTarget"),
            size:               wgpu::Extent3d { width: width as _, height: height as _, depth_or_array_layers: 1 },
            mip_level_count:    1,
            sample_count:       1,
            dimension:          wgpu::TextureDimension::D2,
            format:             wgpu::TextureFormat::Rgba8Unorm,
            usage:              wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT
        });

        let target_texture = Arc::new(target_texture);

        // Create a renderer that will write to this texture
        let renderer = WgpuRenderer::from_texture(Arc::clone(&self.device), Arc::clone(&self.queue), Arc::clone(&target_texture), Arc::clone(&self.adapter), wgpu::TextureFormat::Rgba8Unorm, (width as _, height as _));

        // Build the render target
        WgpuOffscreenRenderTarget {
            device:     Arc::clone(&self.device),
            queue:      Arc::clone(&self.queue),
            size:       (width as _, height as _),
            texture:    target_texture,
            renderer:   renderer,
        }
    }
}

impl OffscreenRenderTarget for WgpuOffscreenRenderTarget {
    ///
    /// Sends render actions to this offscreen render target
    ///
    fn render<ActionIter: IntoIterator<Item=RenderAction>>(&mut self, actions: ActionIter) {
        unimplemented!("render")
    }

    ///
    /// Consumes this render target and returns the realized pixels as a byte array
    ///
    fn realize(self) -> Vec<u8> {
        unimplemented!("realize")
    }
}
