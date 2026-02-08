use std::any::Any;

use anyhow::{Result, anyhow};
use bytemuck::Pod;

use crate::{CursorLayoutView, NodeId, NodeKind, ShaderOffset};

pub struct ShaderCursor {
    view: CursorLayoutView,
    object: Box<dyn ShaderObject>,
}

#[derive(Clone, Copy, Debug)]
struct BindTarget {
    resolved_node: Option<NodeId>,
    expected_class: Option<DescriptorClass>,
}

impl ShaderCursor {
    pub fn new(view: CursorLayoutView, object: Box<dyn ShaderObject>) -> Self {
        Self { view, object }
    }

    pub fn field(self, name: &str) -> Result<Self> {
        let view = self
            .view
            .field(name)
            .ok_or_else(|| anyhow!("field '{name}' not found or current node is not a struct"))?;

        Ok(Self {
            view,
            object: self.object,
        })
    }

    pub fn element(self, index: usize) -> Result<Self> {
        let view = self.view.element(index).ok_or_else(|| {
            anyhow!("array element index {index} out of bounds or current node is not an array")
        })?;

        Ok(Self {
            view,
            object: self.object,
        })
    }

    pub fn into_object(self) -> Box<dyn ShaderObject> {
        self.object
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.object.write(self.view.base, bytes)
    }

    pub fn write_pod<T: Pod>(&mut self, pod: &T) -> Result<()> {
        self.write_bytes(bytemuck::bytes_of(pod))
    }

    // TODO:
    // - write_bool
    // - write_u8  ...
    // - write_u32 ...
    // - write_f32 ... etc

    pub fn bind(&mut self, object: &dyn ShaderResource) -> Result<()> {
        let bind_target = self.bind_target()?;

        let descriptor = object.descriptor();
        Self::validate_descriptor_profile(bind_target, descriptor.as_ref(), false)?;

        let block = self
            .object
            .as_shader_block()
            .ok_or_else(|| anyhow!("current object does not support descriptor binding"))?;

        block.bind(self.view.base, descriptor)
    }

    pub fn bind_and_resolve(
        &mut self,
        object: Box<dyn ShaderWritableResource>,
    ) -> Result<ShaderCursor> {
        let bind_target = self.bind_target()?;
        let root_node = bind_target.resolved_node.ok_or_else(|| {
            anyhow!("current cursor target is bindable but has no writable element layout")
        })?;

        let descriptor = object.descriptor();
        Self::validate_descriptor_profile(bind_target, descriptor.as_ref(), true)?;

        let block = self
            .object
            .as_shader_block()
            .ok_or_else(|| anyhow!("current object does not support descriptor binding"))?;

        block.bind(self.view.base, descriptor)?;

        let view = CursorLayoutView {
            layout: self.view.layout.clone(),
            node: root_node,
            // Resolved cursors are object-local by design.
            base: ShaderOffset::default(),
        };

        Ok(Self::new(view, object.into_shader_object()))
    }

    fn bind_target(&self) -> Result<BindTarget> {
        let node = self
            .view
            .layout
            .node(self.view.node)
            .ok_or_else(|| anyhow!("cursor points to an invalid layout node"))?;

        match &node.kind {
            NodeKind::Resource { element } => Ok(BindTarget {
                resolved_node: *element,
                expected_class: if element.is_some() {
                    Some(DescriptorClass::StorageBuffer)
                } else {
                    None
                },
            }),
            NodeKind::ConstantBuffer { element } => Ok(BindTarget {
                resolved_node: Some(*element),
                expected_class: Some(DescriptorClass::UniformBuffer),
            }),
            NodeKind::ParameterBlock { element, .. } => Ok(BindTarget {
                resolved_node: Some(*element),
                expected_class: None,
            }),
            _ => Err(anyhow!(
                "current cursor target is not a bindable resource node"
            )),
        }
    }

    fn validate_descriptor_profile(
        bind_target: BindTarget,
        descriptor: &dyn ResourceDescriptor,
        require_resolve_compatible: bool,
    ) -> Result<()> {
        let profile = descriptor.profile();

        if let Some(expected_class) = bind_target.expected_class {
            if profile.class != expected_class {
                return Err(anyhow!(
                    "descriptor class mismatch: expected {:?}, got {:?}",
                    expected_class,
                    profile.class
                ));
            }
        }

        if require_resolve_compatible
            && !matches!(
                profile.class,
                DescriptorClass::UniformBuffer | DescriptorClass::StorageBuffer
            )
        {
            return Err(anyhow!(
                "bind_and_resolve requires a buffer descriptor, got {:?}",
                profile.class
            ));
        }

        Ok(())
    }
}

pub trait ShaderObject {
    fn as_shader_block(&mut self) -> Option<&mut dyn ShaderParameterBlock>;
    fn write(&mut self, offset: ShaderOffset, bytes: &[u8]) -> Result<()>;
}

pub trait ShaderParameterBlock: ShaderObject {
    fn bind(&mut self, offset: ShaderOffset, descriptor: Box<dyn ResourceDescriptor>)
    -> Result<()>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorClass {
    UniformBuffer,
    StorageBuffer,
    SampledImage,
    StorageImage,
    Sampler,
    AccelerationStructure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescriptorProfile {
    pub class: DescriptorClass,
    pub writable: bool,
}

pub trait ResourceDescriptor: Any + Send + Sync {
    fn profile(&self) -> DescriptorProfile;
    fn as_any(&self) -> &dyn Any;
}

pub trait ShaderResource {
    fn descriptor(&self) -> Box<dyn ResourceDescriptor>;
}

pub trait ShaderWritableResource: ShaderResource + ShaderObject {
    fn into_shader_object(self: Box<Self>) -> Box<dyn ShaderObject>;
}

impl<T: ShaderObject + ShaderResource + 'static> ShaderWritableResource for T {
    fn into_shader_object(self: Box<Self>) -> Box<dyn ShaderObject> {
        self
    }
}
