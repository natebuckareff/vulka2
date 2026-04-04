use std::sync::Arc;

use anyhow::{Result, anyhow};
use slang::SlangShaderStage;
use vulkanalia::vk;

use crate::gpu::{
    CommandBuffer, Device, OwnedGraphicsPipeline, Pipeline, PipelineLayout, RenderingLayout,
    ShaderModule, VulkanResource,
};

#[derive(Default)]
pub struct GraphicsPipelineBuilder {
    device: Option<Arc<Device>>,
    flags: vk::PipelineCreateFlags,
    shader_stages: Vec<ShaderStage>,
    topology: Option<vk::PrimitiveTopology>,
    primitive_restart: bool,
    viewports: Option<ViewportState>,
    depth_clamp: bool,
    rasterizer_discard: bool,
    polygon_mode: Option<vk::PolygonMode>,
    cull_mode: Option<vk::CullModeFlags>,
    front_face: Option<vk::FrontFace>,
    depth_bias: Option<DepthBias>,
    line_width: Option<f32>,
    multisample: Option<MultisampleState>,
    depth_stencil: Option<DepthStencilState>,
    color_blend: Option<ColorBlendState>,
    layout: Option<Arc<PipelineLayout>>,
    rendering: Option<Arc<RenderingLayout>>,
    base_pipeline: Option<Arc<GraphicsPipeline>>,
}

impl GraphicsPipelineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn device(mut self, device: Arc<Device>) -> Self {
        self.device = Some(device);
        self
    }

    pub fn allow_derivatives(mut self) -> Self {
        self.flags |= vk::PipelineCreateFlags::ALLOW_DERIVATIVES;
        self
    }

    pub fn stage(self, module: Arc<ShaderModule>) -> Self {
        self.stage_with_flags(module, Default::default())
    }

    pub fn stage_with_flags(
        mut self,
        module: Arc<ShaderModule>,
        flags: vk::PipelineShaderStageCreateFlags,
    ) -> Self {
        self.shader_stages.push(ShaderStage { module, flags });
        self
    }

    pub fn topology(mut self, topology: vk::PrimitiveTopology) -> Self {
        self.topology = Some(topology);
        self
    }

    pub fn primitive_restart(mut self, enabled: bool) -> Self {
        self.primitive_restart = enabled;
        self
    }

    pub fn viewports(mut self, viewports: Viewports) -> Self {
        self.viewports = Some(viewports.state);
        self
    }

    pub fn depth_clamp(mut self, enabled: bool) -> Self {
        self.depth_clamp = enabled;
        self
    }

    pub fn rasterizer_discard(mut self, enabled: bool) -> Self {
        self.rasterizer_discard = enabled;
        self
    }

    pub fn polygon_mode(mut self, mode: vk::PolygonMode) -> Self {
        self.polygon_mode = Some(mode);
        self
    }

    // TODO: validate should be a single flag value?
    pub fn cull_mode(mut self, mode: vk::CullModeFlags) -> Self {
        self.cull_mode = Some(mode);
        self
    }

    // TODO: validate should be a single flag value?
    pub fn front_face(mut self, front_face: vk::FrontFace) -> Self {
        self.front_face = Some(front_face);
        self
    }

    pub fn depth_bias(mut self, depth_bias: Option<DepthBias>) -> Self {
        self.depth_bias = depth_bias;
        self
    }

    pub fn line_width(mut self, line_width: f32) -> Self {
        self.line_width = Some(line_width);
        self
    }

    pub fn multisample(mut self, multisample: MultisampleState) -> Self {
        self.multisample = Some(multisample);
        self
    }

    pub fn depth_stencil(mut self, depth_stencil: DepthStencilState) -> Self {
        self.depth_stencil = Some(depth_stencil);
        self
    }

    pub fn color_blend(mut self, color_blend: ColorBlendState) -> Self {
        self.color_blend = Some(color_blend);
        self
    }

    pub fn layout(mut self, layout: Arc<PipelineLayout>) -> Self {
        self.layout = Some(layout);
        self
    }

    // TODO: more dynamic state, using the same type pattern as viewports, to
    // avoid PSO explosion

    pub fn rendering(mut self, rendering: Arc<RenderingLayout>) -> Self {
        self.rendering = Some(rendering);
        self
    }

    pub fn base_pipeline(mut self, pipeline: Arc<GraphicsPipeline>) -> Result<Self> {
        if !pipeline
            .flags
            .contains(vk::PipelineCreateFlags::ALLOW_DERIVATIVES)
        {
            return Err(anyhow!("base pipeline does not have allow derivatives set"));
        }
        self.flags |= vk::PipelineCreateFlags::DERIVATIVE;
        self.base_pipeline = Some(pipeline);
        Ok(self)
    }

    pub fn build(self) -> Result<GraphicsPipeline> {
        use vulkanalia::prelude::v1_0::*;

        let Some(device) = &self.device else {
            // TODO: standardize builder error messages
            return Err(anyhow!("no device provided"));
        };

        let Some(viewports) = self.viewports else {
            return Err(anyhow!("no viewport state provided"));
        };

        let Some(topology) = self.topology else {
            return Err(anyhow!("no primitive topology provided"));
        };

        let Some(polygon_mode) = self.polygon_mode else {
            return Err(anyhow!("no polygon mode provided"));
        };

        let Some(cull_mode) = self.cull_mode else {
            return Err(anyhow!("no cull mode provided"));
        };

        let Some(front_face) = self.front_face else {
            return Err(anyhow!("no front face provided"));
        };

        let Some(layout) = self.layout else {
            return Err(anyhow!("no pipeline layout provided"));
        };

        let Some(rendering) = self.rendering else {
            return Err(anyhow!("no rendering layout provided"));
        };

        let mut stage_infos = Vec::with_capacity(self.shader_stages.len());
        let mut has_vertex_stage = false;
        let mut has_fragment_stage = false;

        for shader_stage in &self.shader_stages {
            let stage = match &shader_stage.module.stage() {
                SlangShaderStage::Vertex => {
                    if has_vertex_stage {
                        return Err(anyhow!(
                            "multiple vertex stages are invalid for a graphics pipeline"
                        ));
                    }
                    has_vertex_stage = true;
                    vk::ShaderStageFlags::VERTEX
                }
                SlangShaderStage::Fragment => {
                    if has_fragment_stage {
                        return Err(anyhow!(
                            "multiple fragment stages are invalid for a graphics pipeline"
                        ));
                    }
                    has_fragment_stage = true;
                    vk::ShaderStageFlags::FRAGMENT
                }
                SlangShaderStage::Compute => {
                    return Err(anyhow!("compute shader is invalid for a graphics pipeline"));
                }
            };

            let name = shader_stage.module.entrypoint().to_bytes_with_nul();

            // TODO: specialization?
            let info = vk::PipelineShaderStageCreateInfo::builder()
                .flags(shader_stage.flags)
                .stage(stage)
                .module(unsafe { *shader_stage.module.owned().raw() })
                .name(name);

            stage_infos.push(info);
        }

        if !has_vertex_stage {
            return Err(anyhow!("graphics pipeline requires a vertex stage"));
        }

        // empty but present, when using vertex pulling
        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&[] as &[vk::VertexInputBindingDescription])
            .vertex_attribute_descriptions(&[] as &[vk::VertexInputAttributeDescription]);

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(topology)
            .primitive_restart_enable(self.primitive_restart);

        let mut dynamic = vec![];

        let mut vk_viewports: Vec<vk::Viewport> = vec![];
        let mut vk_scissors: Vec<vk::Rect2D> = vec![];

        let viewport_state = match viewports {
            ViewportState::Fixed(viewports) => {
                vk_viewports = viewports.iter().map(|v| v.transform).collect();
                vk_scissors = viewports.iter().map(|v| v.scissor).collect();
                vk::PipelineViewportStateCreateInfo::builder()
                    .viewports(&vk_viewports)
                    .scissors(&vk_scissors)
            }
            ViewportState::Dynamic { count } => {
                dynamic.push(vk::DynamicState::VIEWPORT);
                dynamic.push(vk::DynamicState::SCISSOR);
                vk::PipelineViewportStateCreateInfo::builder()
                    .viewport_count(count)
                    .scissor_count(count)
            }
        };

        let mut rasterization_state = vk::PipelineRasterizationStateCreateInfo::builder()
            .depth_clamp_enable(self.depth_clamp)
            .rasterizer_discard_enable(self.rasterizer_discard)
            .polygon_mode(polygon_mode)
            .cull_mode(cull_mode)
            .front_face(front_face)
            .depth_bias_enable(self.depth_bias.is_some())
            .line_width(self.line_width.unwrap_or(1.0));

        if let Some(depth_bias) = self.depth_bias {
            rasterization_state = rasterization_state
                .depth_bias_constant_factor(depth_bias.constant_factor)
                .depth_bias_clamp(depth_bias.clamp)
                .depth_bias_slope_factor(depth_bias.slope_factor);
        };

        let multisample = self.multisample.unwrap_or_default();
        let multisample_state = {
            let samples = rendering.samples.flags();

            let mut info = vk::PipelineMultisampleStateCreateInfo::builder()
                .rasterization_samples(samples)
                .alpha_to_coverage_enable(multisample.alpha_to_coverage)
                .alpha_to_one_enable(multisample.alpha_to_one);

            if let Some(min_sample_shading) = multisample.min_sample_shading {
                info = info
                    .sample_shading_enable(true)
                    .min_sample_shading(min_sample_shading);
            } else {
                info = info.sample_shading_enable(false);
            }

            if let Some(sample_masks) = &multisample.sample_masks {
                if sample_masks.len() != rendering.samples.sample_mask_count() {
                    return Err(anyhow!("sample mask count does not match rendering layout"));
                }
                info = info.sample_mask(&sample_masks);
            }

            info
        };

        let depth_stencil_state = &self.depth_stencil.as_ref().map(|depth_stencil| {
            let mut info = vk::PipelineDepthStencilStateCreateInfo::builder()
                .flags(depth_stencil.flags)
                .depth_test_enable(depth_stencil.depth_test.is_some())
                .stencil_test_enable(depth_stencil.stencil_test.is_some());

            if let Some(depth) = &depth_stencil.depth_test {
                info = info
                    .depth_write_enable(depth.write_enabled)
                    .depth_compare_op(depth.compare_op)
                    .depth_bounds_test_enable(depth.bounds_test.is_some());

                if let Some(bounds) = &depth.bounds_test {
                    info = info
                        .min_depth_bounds(bounds.min)
                        .max_depth_bounds(bounds.max);
                }
            }

            if let Some(stencil) = &depth_stencil.stencil_test {
                info = info.front(stencil.front).back(stencil.back);
            }

            info
        });

        let mut vk_attachments = vec![];

        let color_blend_state = &self
            .color_blend
            .as_ref()
            .map(|color_blend| {
                if color_blend.attachments.len() != rendering.color_formats.len() {
                    return Err(anyhow!(
                        "color blend attachment count does not match rendering layout"
                    ));
                }

                vk_attachments = Vec::with_capacity(color_blend.attachments.len());
                for attachment in &color_blend.attachments {
                    let info = match attachment {
                        ColorAttachmentBlend::Disabled { color_write_mask } => {
                            vk::PipelineColorBlendAttachmentState::builder()
                                .blend_enable(false)
                                .color_write_mask(*color_write_mask)
                        }
                        ColorAttachmentBlend::Enabled(state) => {
                            vk::PipelineColorBlendAttachmentState::builder()
                                .blend_enable(true)
                                .src_color_blend_factor(state.src_color_blend_factor)
                                .dst_color_blend_factor(state.dst_color_blend_factor)
                                .color_blend_op(state.color_blend_op)
                                .src_alpha_blend_factor(state.src_alpha_blend_factor)
                                .dst_alpha_blend_factor(state.dst_alpha_blend_factor)
                                .alpha_blend_op(state.alpha_blend_op)
                                .color_write_mask(state.color_write_mask)
                        }
                    };
                    vk_attachments.push(info);
                }

                let mut info = vk::PipelineColorBlendStateCreateInfo::builder()
                    .flags(color_blend.flags)
                    .logic_op_enable(color_blend.logic_op.is_some())
                    .attachments(&vk_attachments)
                    .blend_constants(color_blend.blend_constants);

                if let Some(logic_op) = color_blend.logic_op {
                    info = info.logic_op(logic_op);
                }

                Ok(info)
            })
            .transpose()?;

        let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(&dynamic);

        let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
            .view_mask(rendering.view_mask())
            .color_attachment_formats(&rendering.color_formats);

        if let Some(format) = rendering.depth_format {
            rendering_info = rendering_info.depth_attachment_format(format);
        }

        if let Some(format) = rendering.stencil_format {
            rendering_info = rendering_info.stencil_attachment_format(format);
        }

        let mut pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .push_next(&mut rendering_info)
            .stages(&stage_infos)
            .vertex_input_state(&vertex_input_state)
            .input_assembly_state(&input_assembly_state)
            .viewport_state(&viewport_state)
            .multisample_state(&multisample_state)
            .rasterization_state(&rasterization_state)
            .dynamic_state(&dynamic_state)
            .layout(unsafe { *layout.owned().raw() })
            .render_pass(vk::RenderPass::null())
            .subpass(0);

        if let Some(state) = &depth_stencil_state {
            pipeline_info = pipeline_info.depth_stencil_state(state);
        }

        if let Some(state) = &color_blend_state {
            pipeline_info = pipeline_info.color_blend_state(state);
        }

        if let Some(base_pipeline) = self.base_pipeline {
            let handle = unsafe { *base_pipeline.owned.raw() };
            pipeline_info = pipeline_info.base_pipeline_handle(handle);
        } else {
            pipeline_info = pipeline_info.base_pipeline_handle(vk::Pipeline::null());
        }

        pipeline_info = pipeline_info.flags(self.flags);

        let owned = OwnedGraphicsPipeline::new(device.handle().clone(), pipeline_info)?;

        Ok(GraphicsPipeline::new(self.flags, owned, layout, rendering))
    }
}

struct ShaderStage {
    module: Arc<ShaderModule>,
    flags: vk::PipelineShaderStageCreateFlags,
}

pub struct Viewports {
    state: ViewportState,
}

impl Viewports {
    pub fn fixed(viewports: Box<[Viewport]>) -> Self {
        Self {
            state: ViewportState::Fixed(viewports),
        }
    }

    pub fn dynamic(count: u32) -> Result<Self> {
        if count == 0 {
            return Err(anyhow!("invalid viewport count"));
        }
        Ok(Self {
            state: ViewportState::Dynamic { count },
        })
    }
}

enum ViewportState {
    Fixed(Box<[Viewport]>),
    Dynamic { count: u32 },
}

pub struct Viewport {
    pub transform: vk::Viewport,
    pub scissor: vk::Rect2D,
}

pub struct DepthBias {
    pub constant_factor: f32,
    pub clamp: f32,
    pub slope_factor: f32,
}

pub struct MultisampleState {
    pub min_sample_shading: Option<f32>,
    pub sample_masks: Option<Box<[vk::SampleMask]>>,
    pub alpha_to_coverage: bool,
    pub alpha_to_one: bool,
}

impl Default for MultisampleState {
    fn default() -> Self {
        Self {
            min_sample_shading: None,
            sample_masks: None,
            alpha_to_coverage: false,
            alpha_to_one: false,
        }
    }
}

pub struct DepthStencilState {
    pub flags: vk::PipelineDepthStencilStateCreateFlags,
    pub depth_test: Option<DepthTestState>,
    pub stencil_test: Option<StencilTestState>,
}

pub struct DepthTestState {
    pub write_enabled: bool,
    pub compare_op: vk::CompareOp,
    pub bounds_test: Option<DepthBounds>,
}

pub struct DepthBounds {
    pub min: f32,
    pub max: f32,
}

pub struct StencilTestState {
    pub front: vk::StencilOpState,
    pub back: vk::StencilOpState,
}

pub struct ColorBlendState {
    pub flags: vk::PipelineColorBlendStateCreateFlags,
    pub logic_op: Option<vk::LogicOp>,
    pub attachments: Vec<ColorAttachmentBlend>,
    pub blend_constants: [f32; 4], // TODO: glam Vec4?
}

pub enum ColorAttachmentBlend {
    Disabled {
        color_write_mask: vk::ColorComponentFlags,
    },
    Enabled(ColorBlendAttachmentState),
}

pub struct ColorBlendAttachmentState {
    pub src_color_blend_factor: vk::BlendFactor,
    pub dst_color_blend_factor: vk::BlendFactor,
    pub color_blend_op: vk::BlendOp,
    pub src_alpha_blend_factor: vk::BlendFactor,
    pub dst_alpha_blend_factor: vk::BlendFactor,
    pub alpha_blend_op: vk::BlendOp,
    pub color_write_mask: vk::ColorComponentFlags,
}

pub struct GraphicsPipeline {
    flags: vk::PipelineCreateFlags,
    owned: OwnedGraphicsPipeline,
    layout: Arc<PipelineLayout>,
    rendering: Arc<RenderingLayout>,
}

impl GraphicsPipeline {
    fn new(
        flags: vk::PipelineCreateFlags,
        owned: OwnedGraphicsPipeline,
        layout: Arc<PipelineLayout>,
        rendering: Arc<RenderingLayout>,
    ) -> Self {
        Self {
            flags,
            owned,
            layout,
            rendering,
        }
    }

    pub(crate) fn owned(&self) -> &OwnedGraphicsPipeline {
        &self.owned
    }

    pub fn layout(&self) -> &Arc<PipelineLayout> {
        &self.layout
    }

    pub fn rendering(&self) -> &Arc<RenderingLayout> {
        &self.rendering
    }
}

impl Pipeline for GraphicsPipeline {
    fn bind(&self, cmdbuf: &mut CommandBuffer) {
        todo!()
    }
}
