//! Expose render functionality on different types through traits.

use std::{borrow::Cow, marker::PhantomData};

use hashbrown::HashMap;

use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, BindGroupLayout, BlendState, Buffer, BufferUsages, Color, ColorTargetState,
    ColorWrites, CommandEncoder, Device, FragmentState, FrontFace, IndexFormat, LoadOp,
    MultisampleState, Operations, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    ShaderModuleDescriptor, ShaderSource, StoreOp, TextureView, VertexState,
};

use crate::graphics::state::PREFERRED_TEXTURE_FORMAT;

use super::{
    atlas::Atlas, data::TexturedVertex, instance::Instances, texture::TextureRef,
    uniform::UniformState, Render,
};

// TODO: batch all textures split by sampler arrays with the max value get from adapter

/// Simple render state holding buffers and instances required for rendering somethging.
pub(crate) struct RenderState<R: Render> {
    /// Pipeline of the rendering itself.
    render_pipeline: RenderPipeline,
    /// GPU buffer reference to the vertices.
    vertex_buffer: Buffer,
    /// GPU buffer reference to the indices.
    index_buffer: Buffer,
    /// GPU buffer reference to the instances.
    instance_buffer: Buffer,
    /// Hold the type so we can't have mismatching calls.
    _phantom: PhantomData<R>,
}

impl<R: Render> RenderState<R> {
    /// Create the state by calling the [`Render`] trait implementations on the type.
    pub(crate) fn new(
        device: &Device,
        diffuse_texture_bind_group_layout: &BindGroupLayout,
        screen_info_bind_group_layout: &BindGroupLayout,
    ) -> Self
    where
        R: Render,
    {
        log::debug!("Creating custom rendering component");

        // Create a new render pipeline first
        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Component Render Pipeline Layout"),
            bind_group_layouts: &[
                diffuse_texture_bind_group_layout,
                screen_info_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        // Load the shaders from disk
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Diffuse Texture Shader"),
            source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("./shaders/texture.wgsl"))),
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Component Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: VertexState {
                buffers: &[TexturedVertex::descriptor(), Instances::descriptor()],
                module: &shader,
                entry_point: "vs_main",
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format: PREFERRED_TEXTURE_FORMAT,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: R::topology(),
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                // Irrelevant since we disable culling
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                // How many samples the pipeline will use
                count: 1,
                // Use all masks
                mask: !0,
                // Disable anti-aliasing
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Create the initial empty instance buffer, will be resized by the render call
        let instance_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: &[],
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST | BufferUsages::STORAGE,
        });

        // Create the vertex buffer
        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: &[],
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });

        // Create the index buffer
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: &[],
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
        });

        Self {
            render_pipeline,
            vertex_buffer,
            instance_buffer,
            index_buffer,
            _phantom: PhantomData,
        }
    }

    /// Render all instances from the type with the specified texture.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render(
        &mut self,
        target: &mut R,
        encoder: &mut CommandEncoder,
        view: &TextureView,
        queue: &Queue,
        device: &Device,
        screen_size_bind_group: &BindGroup,
        texture_atlas: &Atlas,
        background_color: Color,
    ) {
        // Target will be rendered
        target.pre_render();

        // Get the instances
        let (instances_bytes, instances_len) = {
            // We could simplify the scope here by implementing `Render::instances()` but that means implementing another method for each trait implementor

            let instances = target.instances_mut();
            if instances.is_empty() {
                // Nothing to render when there's no instances
                return;
            }

            // Construct the bytes of the instances to upload
            (instances.bytes(), instances.len())
        };

        // Upload the instance buffer
        if instances_bytes.len() as u64 <= self.instance_buffer.size() {
            // We still fit in the buffer, we don't have to resize it
            queue.write_buffer(&self.instance_buffer, 0, instances_bytes);
        } else {
            // We have more instances than the buffer size, recreate the buffer

            log::debug!(
                "Previous instance buffer is too small, rescaling to {} items",
                instances_len
            );

            self.instance_buffer.destroy();
            self.instance_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Instance Buffer"),
                contents: instances_bytes,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST | BufferUsages::STORAGE,
            });
        }

        // Upload the buffers if dirty
        if target.is_dirty() {
            profiling::scope!("Re-uploading buffers for dirty texture");

            log::debug!(
                "Custom rendering component is dirty, re-uploading vertex and index buffers"
            );

            // Recreate the vertex buffer
            self.vertex_buffer.destroy();
            self.vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(target.vertices()),
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            });

            // Re upload the indices
            self.index_buffer.destroy();
            self.index_buffer = device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(target.indices()),
                usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            });

            // Target is not dirty anymore
            target.mark_clean();
        }

        // Allow components to clean up, doesn't matter even though it's technically not rendered yet
        target.post_render();

        // Start the render pass
        let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Component Render Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(background_color),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Set our pipeline
        render_pass.set_pipeline(&self.render_pipeline);

        // Bind the atlas texture
        render_pass.set_bind_group(0, &texture_atlas.bind_group, &[]);
        // Bind the atlas texture info
        render_pass.set_bind_group(1, &texture_atlas.rects.bind_group, &[]);

        // Bind the screen size
        render_pass.set_bind_group(2, screen_size_bind_group, &[]);

        // Set the target vertices
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        // Set the instances
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        // Set the target indices
        render_pass.set_index_buffer(self.index_buffer.slice(..), IndexFormat::Uint16);

        // Draw the instances
        render_pass.draw_indexed(target.range(), 0, 0..instances_len as u32);
    }
}
