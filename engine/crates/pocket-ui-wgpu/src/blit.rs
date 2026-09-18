//! Fullscreen-triangle blit: composite one texture onto another target.
//!
//! R1 presentation policy (PicoView #63 Corrective B):
//! - **Exact** — retained target size == swapchain size. Nearest, no blur.
//! - **Transient** — sizes differ (OS window moved ahead of the worker).
//!   Linear bridge only until an exact-size frame arrives.
//!
//! A cached blit for one Target must support both policies; filter is chosen
//! per presentation, not frozen at first use.

use pocket3d::gpu::Gpu;

const BLIT_WGSL: &str = r#"
struct VsOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f };

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(i % 2u) * 4 - 1);
    let y = f32(i32(i / 2u) * 4 - 1);
    out.pos = vec4f(x, y, 0.0, 1.0);
    out.uv = vec2f((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
    return textureSample(tex, samp, in.uv);
}
"#;

/// Retained-target presentation filter policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlitFilter {
    /// Settled exact frame: `target.size == swapchain.size`.
    Exact,
    /// Transient size bridge while an exact frame is pending.
    Transient,
}

impl BlitFilter {
    pub fn wgpu_filter(self) -> wgpu::FilterMode {
        match self {
            BlitFilter::Exact => wgpu::FilterMode::Nearest,
            BlitFilter::Transient => wgpu::FilterMode::Linear,
        }
    }

    /// Select policy from retained source vs live swapchain destination.
    /// Zero-sized destinations are transient (nothing exact to present).
    pub fn select(source: (u32, u32), destination: (u32, u32)) -> BlitFilter {
        if source.0 > 0
            && source.1 > 0
            && source.0 == destination.0
            && source.1 == destination.1
        {
            BlitFilter::Exact
        } else {
            BlitFilter::Transient
        }
    }
}

pub struct Blit {
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
}

impl Blit {
    /// A blit from `src` into targets of `format`. `blend` enables standard
    /// alpha blending (overlay compositing); disable it for opaque copies.
    pub fn new(
        gpu: &Gpu,
        src: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        filter: wgpu::FilterMode,
        blend: bool,
    ) -> Blit {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("pocket-ui blit"),
                source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
            });
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("pocket-ui blit layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pocket-ui blit sampler"),
            mag_filter: filter,
            min_filter: filter,
            ..Default::default()
        });
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pocket-ui blit bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pl = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("pocket-ui blit pl"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("pocket-ui blit pipeline"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
        Blit { pipeline, bind }
    }

    /// Draw the blit inside an existing render pass (set the viewport first
    /// for letterboxed integer scaling).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Cached presentation blit pair for one retained Target.
///
/// Both policies are prebuilt at first use. Per present, select Exact or
/// Transient from live sizes — do not freeze the filter on the Target.
pub struct BlitSet {
    exact: Blit,
    transient: Blit,
}

impl BlitSet {
    pub fn new(gpu: &Gpu, src: &wgpu::TextureView, format: wgpu::TextureFormat, blend: bool) -> Self {
        Self {
            exact: Blit::new(gpu, src, format, BlitFilter::Exact.wgpu_filter(), blend),
            transient: Blit::new(
                gpu,
                src,
                format,
                BlitFilter::Transient.wgpu_filter(),
                blend,
            ),
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, filter: BlitFilter) {
        match filter {
            BlitFilter::Exact => self.exact.draw(pass),
            BlitFilter::Transient => self.transient.draw(pass),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BlitFilter;

    /// Corrective B regression: same Target, changing surface size switches
    /// policy; returning to exact size uses Exact again.
    #[test]
    fn exact_transient_policy_survives_target_identity() {
        let target = (1000u32, 700u32);

        // Exact now.
        assert_eq!(BlitFilter::select(target, (1000, 700)), BlitFilter::Exact);
        // SAME target -> surface moved ahead -> Transient Linear.
        assert_eq!(BlitFilter::select(target, (1017, 711)), BlitFilter::Transient);
        // SAME target again at exact size -> Exact (Nearest), not stuck Linear.
        assert_eq!(BlitFilter::select(target, (1000, 700)), BlitFilter::Exact);
        // Downscale/upscale mismatch is also transient.
        assert_eq!(BlitFilter::select((1920, 1280), (960, 640)), BlitFilter::Transient);
        assert_eq!(BlitFilter::select((0, 0), (100, 100)), BlitFilter::Transient);
    }

    #[test]
    fn exact_filter_is_nearest_transient_is_linear() {
        assert_eq!(BlitFilter::Exact.wgpu_filter(), wgpu::FilterMode::Nearest);
        assert_eq!(
            BlitFilter::Transient.wgpu_filter(),
            wgpu::FilterMode::Linear
        );
    }
}
