use anyhow::{Result, anyhow};
use compact_str::CompactString;
use std::sync::Arc;

use crate::{
    DescriptorSet, ElementCount, ShaderLayout, SlangShaderStage, Type, VarLayout,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct NodeId(pub usize);

#[derive(Clone, Copy, Debug, Default)]
pub struct ShaderOffset {
    pub bytes: usize,
    pub set: usize,
    pub binding_range: i64,
    pub array_index: usize,
    pub varying_input: usize,
}

#[derive(Debug)]
pub struct Root {
    pub node: NodeId,
    pub base: ShaderOffset,
}

#[derive(Debug)]
pub struct EntrypointRoot {
    pub name: CompactString,
    pub stage: SlangShaderStage,
    pub root: Root,
}

#[derive(Debug)]
pub struct FieldEdge {
    pub name: CompactString,
    pub offset_bytes: usize,
    pub offset_set: usize,
    pub offset_binding_range: i64,
    pub offset_varying_input: usize,
    pub child: NodeId,
}

#[derive(Debug)]
pub enum NodeKind {
    Struct {
        fields: Vec<FieldEdge>,
    },
    Array {
        count: ElementCount,
        stride_bytes: usize,
        stride_binding_range: i64,
        stride_varying_input: usize,
        element: NodeId,
    },
    ParameterBlock {
        descriptor_set: DescriptorSet,
        element: NodeId,
    },
    Resource {
        element: Option<NodeId>,
    },
    ConstantBuffer {
        element: NodeId,
    },
    ScalarLike, // numeric/pointer/etc for now
}

#[derive(Debug)]
pub struct Node {
    pub kind: NodeKind,
}

#[derive(Debug)]
pub struct CursorLayout {
    pub nodes: Vec<Node>,
    pub global: Option<Root>,
    pub entrypoints: Vec<EntrypointRoot>,
}

impl CursorLayout {
    pub fn build(layout: ShaderLayout) -> Result<CursorLayout> {
        LayoutIndexer::build(layout)
    }

    pub fn global_view(self: &Arc<Self>) -> Option<CursorLayoutView> {
        self.global
            .as_ref()
            .map(|root| CursorLayoutView::from_root(Arc::clone(self), root))
    }

    pub fn entrypoint_view(
        self: &Arc<Self>,
        stage: SlangShaderStage,
        name: &str,
    ) -> Option<CursorLayoutView> {
        self.entrypoints
            .iter()
            .find(|entrypoint| entrypoint.stage == stage && entrypoint.name == name)
            .map(|entrypoint| CursorLayoutView::from_root(Arc::clone(self), &entrypoint.root))
    }

    pub fn node(&self, node: NodeId) -> Option<&Node> {
        self.nodes.get(node.0)
    }
}

pub struct LayoutIndexer {
    nodes: Vec<Node>,
}

impl LayoutIndexer {
    fn build(layout: ShaderLayout) -> Result<CursorLayout> {
        let mut b = Self { nodes: Vec::new() };

        let global = layout
            .globals
            .map(|global| b.root_from_var(*global))
            .transpose()?;

        let mut entrypoints = Vec::with_capacity(layout.entrypoints.len());
        for ep in layout.entrypoints {
            if let Some(params) = ep.params {
                entrypoints.push(EntrypointRoot {
                    name: ep.name,
                    stage: ep.stage,
                    root: b.root_from_var(*params)?,
                });
            }
        }

        Ok(CursorLayout {
            nodes: b.nodes,
            global,
            entrypoints,
        })
    }

    fn root_from_var(&mut self, var: VarLayout) -> Result<Root> {
        Ok(Root {
            node: self.intern_type(var.value)?,
            base: ShaderOffset {
                bytes: var.offset_bytes,
                set: var.offset_set,
                binding_range: var.offset_binding_range,
                array_index: 0,
                varying_input: var
                    .varying
                    .as_ref()
                    .map_or(0, |varying| varying.offset_input),
            },
        })
    }

    fn intern_type(&mut self, ty: crate::TypeLayout) -> Result<NodeId> {
        let kind = match ty.ty {
            Type::Struct(s) => {
                let mut fields = Vec::with_capacity(s.fields.len());
                for f in s.fields {
                    fields.push(FieldEdge {
                        name: f.name.unwrap_or_default(),
                        offset_bytes: f.offset_bytes,
                        offset_set: f.offset_set,
                        offset_binding_range: f.offset_binding_range,
                        offset_varying_input: f
                            .varying
                            .as_ref()
                            .map_or(0, |varying| varying.offset_input),
                        child: self.intern_type(f.value)?,
                    });
                }
                NodeKind::Struct { fields }
            }
            Type::Array(a) => NodeKind::Array {
                count: a.count.clone(),
                stride_bytes: ty.stride.bytes,
                stride_binding_range: ty.stride.binding_range,
                stride_varying_input: a
                    .element
                    .size
                    .as_ref()
                    .and_then(|size| size.varying_input)
                    .unwrap_or(0),
                element: self.intern_type(*a.element)?,
            },
            Type::ParameterBlock(pb) => NodeKind::ParameterBlock {
                descriptor_set: pb.descriptor_set,
                element: self.intern_type(*pb.element)?,
            },
            Type::ConstantBuffer(inner) => NodeKind::ConstantBuffer {
                element: self.intern_type(*inner)?,
            },
            Type::Resource(r) => NodeKind::Resource {
                element: r
                    .element
                    .map(|e| self.intern_type(*e))
                    .transpose()?,
            },
            Type::Unknown(k, n) => return Err(anyhow!("unknown type: {k} {n}")),
            _ => NodeKind::ScalarLike,
        };

        let id = NodeId(self.nodes.len());
        self.nodes.push(Node { kind });
        Ok(id)
    }
}

#[derive(Clone, Debug)]
pub struct CursorLayoutView {
    pub layout: Arc<CursorLayout>,
    pub node: NodeId,
    pub base: ShaderOffset,
}

impl CursorLayoutView {
    fn from_root(layout: Arc<CursorLayout>, root: &Root) -> Self {
        Self {
            layout,
            node: root.node,
            base: root.base,
        }
    }

    fn apply_field(&self, edge: &FieldEdge) -> Self {
        Self {
            layout: Arc::clone(&self.layout),
            node: edge.child,
            base: ShaderOffset {
                bytes: self.base.bytes + edge.offset_bytes,
                set: self.base.set + edge.offset_set,
                binding_range: self.base.binding_range + edge.offset_binding_range,
                array_index: self.base.array_index,
                varying_input: self.base.varying_input + edge.offset_varying_input,
            },
        }
    }

    pub fn field(self, name: &str) -> Option<Self> {
        let node = self.layout.node(self.node)?;
        let NodeKind::Struct { fields } = &node.kind else {
            return None;
        };

        let edge = fields.iter().find(|field| field.name == name)?;
        Some(self.apply_field(edge))
    }

    pub fn element(self, index: usize) -> Option<Self> {
        let node = self.layout.node(self.node)?;
        let NodeKind::Array {
            count,
            stride_bytes,
            stride_binding_range,
            stride_varying_input,
            element,
        } = &node.kind
        else {
            return None;
        };

        let array_index = match count {
            ElementCount::Bounded(count) => {
                if index >= *count {
                    return None;
                }
                self.base.array_index * count + index
            }
            ElementCount::Runtime => self.base.array_index + index,
        };

        Some(Self {
            layout: Arc::clone(&self.layout),
            node: *element,
            base: ShaderOffset {
                bytes: self.base.bytes + (index * stride_bytes),
                set: self.base.set,
                binding_range: self.base.binding_range + (index as i64 * stride_binding_range),
                array_index,
                varying_input: self.base.varying_input + (index * stride_varying_input),
            },
        })
    }
}
