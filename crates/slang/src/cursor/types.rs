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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::{CursorLayout, FieldEdge, Node, Root};

    #[derive(Clone, Copy)]
    struct MockDescriptor {
        profile: DescriptorProfile,
    }

    impl ResourceDescriptor for MockDescriptor {
        fn profile(&self) -> DescriptorProfile {
            self.profile
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    struct MockResource {
        profile: DescriptorProfile,
    }

    impl MockResource {
        fn new(class: DescriptorClass, writable: bool) -> Self {
            Self {
                profile: DescriptorProfile { class, writable },
            }
        }
    }

    impl ShaderResource for MockResource {
        fn descriptor(&self) -> Box<dyn ResourceDescriptor> {
            Box::new(MockDescriptor {
                profile: self.profile,
            })
        }
    }

    struct MockWritableResource {
        profile: DescriptorProfile,
    }

    impl MockWritableResource {
        fn new(class: DescriptorClass, writable: bool) -> Self {
            Self {
                profile: DescriptorProfile { class, writable },
            }
        }
    }

    impl ShaderResource for MockWritableResource {
        fn descriptor(&self) -> Box<dyn ResourceDescriptor> {
            Box::new(MockDescriptor {
                profile: self.profile,
            })
        }
    }

    impl ShaderObject for MockWritableResource {
        fn as_shader_block(&mut self) -> Option<&mut dyn ShaderParameterBlock> {
            None
        }

        fn write(&mut self, _offset: ShaderOffset, _bytes: &[u8]) -> Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct BindState {
        calls: Vec<(ShaderOffset, DescriptorProfile)>,
    }

    struct MockParameterObject {
        state: Arc<Mutex<BindState>>,
    }

    impl MockParameterObject {
        fn new(state: Arc<Mutex<BindState>>) -> Self {
            Self { state }
        }
    }

    impl ShaderObject for MockParameterObject {
        fn as_shader_block(&mut self) -> Option<&mut dyn ShaderParameterBlock> {
            Some(self)
        }

        fn write(&mut self, _offset: ShaderOffset, _bytes: &[u8]) -> Result<()> {
            Ok(())
        }
    }

    impl ShaderParameterBlock for MockParameterObject {
        fn bind(
            &mut self,
            offset: ShaderOffset,
            descriptor: Box<dyn ResourceDescriptor>,
        ) -> Result<()> {
            let profile = descriptor.profile();
            self.state
                .lock()
                .expect("lock bind state")
                .calls
                .push((offset, profile));
            Ok(())
        }
    }

    fn view_from_layout(
        nodes: Vec<Node>,
        root_node: NodeId,
        base: ShaderOffset,
    ) -> CursorLayoutView {
        let layout = Arc::new(CursorLayout {
            nodes,
            global: Some(Root {
                node: root_node,
                base,
            }),
            entrypoints: vec![],
        });
        layout.global_view().expect("global view")
    }

    #[test]
    fn field_accumulates_set_offset() {
        let base = ShaderOffset {
            bytes: 10,
            set: 20,
            binding_range: 30,
            array_index: 40,
            varying_input: 50,
        };

        let view = view_from_layout(
            vec![
                Node {
                    kind: NodeKind::Struct {
                        fields: vec![FieldEdge {
                            name: "pb".into(),
                            offset_bytes: 4,
                            offset_set: 1,
                            offset_binding_range: 2,
                            offset_varying_input: 3,
                            child: NodeId(1),
                        }],
                    },
                },
                Node {
                    kind: NodeKind::ScalarLike,
                },
            ],
            NodeId(0),
            base,
        );

        let view = view.field("pb").expect("field exists");
        assert_eq!(view.base.bytes, base.bytes + 4);
        assert_eq!(view.base.set, base.set + 1);
        assert_eq!(view.base.binding_range, base.binding_range + 2);
        assert_eq!(view.base.array_index, base.array_index);
        assert_eq!(view.base.varying_input, base.varying_input + 3);
    }

    #[test]
    fn bind_rejects_descriptor_class_mismatch() {
        let view = view_from_layout(
            vec![
                Node {
                    kind: NodeKind::ConstantBuffer { element: NodeId(1) },
                },
                Node {
                    kind: NodeKind::ScalarLike,
                },
            ],
            NodeId(0),
            ShaderOffset::default(),
        );

        let bind_state = Arc::new(Mutex::new(BindState::default()));
        let mut cursor =
            ShaderCursor::new(view, Box::new(MockParameterObject::new(bind_state.clone())));

        let resource = MockResource::new(DescriptorClass::StorageBuffer, true);
        let err = cursor.bind(&resource).expect_err("expected class mismatch");
        assert!(err.to_string().contains("descriptor class mismatch"));
        assert!(bind_state.lock().expect("lock bind state").calls.is_empty());
    }

    #[test]
    fn bind_accepts_matching_descriptor_class() {
        let bind_offset = ShaderOffset {
            bytes: 11,
            set: 12,
            binding_range: 13,
            array_index: 14,
            varying_input: 15,
        };

        let view = view_from_layout(
            vec![
                Node {
                    kind: NodeKind::ConstantBuffer { element: NodeId(1) },
                },
                Node {
                    kind: NodeKind::ScalarLike,
                },
            ],
            NodeId(0),
            bind_offset,
        );

        let bind_state = Arc::new(Mutex::new(BindState::default()));
        let mut cursor =
            ShaderCursor::new(view, Box::new(MockParameterObject::new(bind_state.clone())));

        let resource = MockResource::new(DescriptorClass::UniformBuffer, false);
        cursor.bind(&resource).expect("bind should succeed");

        let state = bind_state.lock().expect("lock bind state");
        assert_eq!(state.calls.len(), 1);
        let (offset, profile) = state.calls[0];
        assert_eq!(offset.bytes, bind_offset.bytes);
        assert_eq!(offset.set, bind_offset.set);
        assert_eq!(offset.binding_range, bind_offset.binding_range);
        assert_eq!(offset.array_index, bind_offset.array_index);
        assert_eq!(offset.varying_input, bind_offset.varying_input);
        assert_eq!(profile.class, DescriptorClass::UniformBuffer);
    }

    #[test]
    fn bind_and_resolve_rejects_parameter_block_node() {
        let view = view_from_layout(
            vec![
                Node {
                    kind: NodeKind::ParameterBlock {
                        descriptor_set: crate::DescriptorSet {
                            set: None,
                            implicit_ubo: None,
                            binding_ranges: vec![],
                        },
                        element: NodeId(1),
                    },
                },
                Node {
                    kind: NodeKind::ScalarLike,
                },
            ],
            NodeId(0),
            ShaderOffset::default(),
        );

        let bind_state = Arc::new(Mutex::new(BindState::default()));
        let mut cursor =
            ShaderCursor::new(view, Box::new(MockParameterObject::new(bind_state.clone())));

        let object = Box::new(MockWritableResource::new(
            DescriptorClass::SampledImage,
            false,
        ));
        match cursor.bind_and_resolve(object) {
            Ok(_) => panic!("expected parameter-block bind rejection"),
            Err(err) => assert!(
                err.to_string()
                    .contains("current cursor target is not a bindable resource node")
            ),
        }
        assert!(bind_state.lock().expect("lock bind state").calls.is_empty());
    }

    #[test]
    fn bind_rejects_non_bindable_node() {
        let view = view_from_layout(
            vec![Node {
                kind: NodeKind::ScalarLike,
            }],
            NodeId(0),
            ShaderOffset::default(),
        );

        let bind_state = Arc::new(Mutex::new(BindState::default()));
        let mut cursor =
            ShaderCursor::new(view, Box::new(MockParameterObject::new(bind_state.clone())));

        let resource = MockResource::new(DescriptorClass::SampledImage, false);
        let err = cursor
            .bind(&resource)
            .expect_err("non-bindable node should reject bind");
        assert!(
            err.to_string()
                .contains("current cursor target is not a bindable resource node")
        );
        assert!(bind_state.lock().expect("lock bind state").calls.is_empty());
    }

    #[test]
    fn bind_and_resolve_returns_object_local_cursor() {
        let bind_offset = ShaderOffset {
            bytes: 32,
            set: 4,
            binding_range: 7,
            array_index: 2,
            varying_input: 5,
        };

        let view = view_from_layout(
            vec![
                Node {
                    kind: NodeKind::Resource {
                        element: Some(NodeId(1)),
                    },
                },
                Node {
                    kind: NodeKind::ScalarLike,
                },
            ],
            NodeId(0),
            bind_offset,
        );

        let bind_state = Arc::new(Mutex::new(BindState::default()));
        let mut cursor =
            ShaderCursor::new(view, Box::new(MockParameterObject::new(bind_state.clone())));

        let object = Box::new(MockWritableResource::new(
            DescriptorClass::StorageBuffer,
            true,
        ));
        let resolved = cursor
            .bind_and_resolve(object)
            .expect("bind_and_resolve succeeds");

        assert_eq!(resolved.view.node, NodeId(1));
        assert_eq!(resolved.view.base.bytes, 0);
        assert_eq!(resolved.view.base.set, 0);
        assert_eq!(resolved.view.base.binding_range, 0);
        assert_eq!(resolved.view.base.array_index, 0);
        assert_eq!(resolved.view.base.varying_input, 0);

        let state = bind_state.lock().expect("lock bind state");
        assert_eq!(state.calls.len(), 1);
        let (offset, profile) = state.calls[0];
        assert_eq!(offset.bytes, bind_offset.bytes);
        assert_eq!(offset.set, bind_offset.set);
        assert_eq!(offset.binding_range, bind_offset.binding_range);
        assert_eq!(offset.array_index, bind_offset.array_index);
        assert_eq!(offset.varying_input, bind_offset.varying_input);
        assert_eq!(profile.class, DescriptorClass::StorageBuffer);
    }
}
