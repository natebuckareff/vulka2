use anyhow::{Context, Result, anyhow};
use compact_str::CompactString;
use std::sync::Arc;

use crate::{DescriptorSetLayout, ElementCount, ShaderLayout, SlangShaderStage, Type, VarLayout};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct NodeId(usize);

#[derive(Clone, Copy, Debug, Default)]
pub struct ShaderOffset {
    pub set: usize,
    pub binding_range: i64,
    pub array_index: usize,
    pub varying_input: usize,
    pub bytes: usize,
}

#[derive(Debug)]
struct SubTree {
    node: NodeId,
    base: ShaderOffset,
}

#[derive(Debug)]
struct EntrypointRoot {
    name: CompactString,
    stage: SlangShaderStage,
    subtree: SubTree,
}

#[derive(Debug)]
struct FieldEdge {
    name: CompactString,
    offset: ShaderOffset,
    child: NodeId,
}

#[derive(Debug)]
enum Node {
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
        descriptor_set: DescriptorSetLayout,
        element: NodeId,
    },
    Resource {
        element: Option<NodeId>,
    },
    Sampler,
    ConstantBuffer {
        element: NodeId,
    },
    ScalarLike, // numeric/pointer/etc for now
}

#[derive(Debug)]
pub struct ShaderTree {
    nodes: Vec<Node>,
    global: Option<SubTree>,
    entrypoints: Vec<EntrypointRoot>,
}

impl ShaderTree {
    pub fn new(layout: ShaderLayout) -> Result<Arc<ShaderTree>> {
        Ok(Arc::new(LayoutIndexer::build(layout)?))
    }

    pub fn globals(self: &Arc<Self>) -> Result<LayoutCursor> {
        self.global
            .as_ref()
            .map(|root| LayoutCursor::from_subtree(Arc::clone(self), root))
            .context("no globals found")
    }

    pub fn entrypoint(
        self: &Arc<Self>,
        stage: SlangShaderStage,
        name: &str,
    ) -> Result<LayoutCursor> {
        self.entrypoints
            .iter()
            .find(|entrypoint| entrypoint.stage == stage && entrypoint.name == name)
            .map(|entrypoint| LayoutCursor::from_subtree(Arc::clone(self), &entrypoint.subtree))
            .context("entrypoint not found")
    }

    fn node(&self, node: NodeId) -> Option<&Node> {
        self.nodes.get(node.0)
    }
}

struct LayoutIndexer {
    nodes: Vec<Node>,
}

impl LayoutIndexer {
    fn build(layout: ShaderLayout) -> Result<ShaderTree> {
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
                    subtree: b.root_from_var(*params)?,
                });
            }
        }

        Ok(ShaderTree {
            nodes: b.nodes,
            global,
            entrypoints,
        })
    }

    fn root_from_var(&mut self, var: VarLayout) -> Result<SubTree> {
        Ok(SubTree {
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
        let node = match ty.ty {
            Type::Struct(s) => {
                let mut fields = Vec::with_capacity(s.fields.len());
                for f in s.fields {
                    fields.push(FieldEdge {
                        name: f.name.unwrap_or_default(),
                        offset: ShaderOffset {
                            set: f.offset_set,
                            binding_range: f.offset_binding_range,
                            array_index: 0,
                            varying_input: f
                                .varying
                                .as_ref()
                                .map_or(0, |varying| varying.offset_input),
                            bytes: f.offset_bytes,
                        },
                        child: self.intern_type(f.value)?,
                    });
                }
                Node::Struct { fields }
            }
            Type::Array(a) => Node::Array {
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
            Type::ParameterBlock(pb) => Node::ParameterBlock {
                descriptor_set: pb.descriptor_set,
                element: self.intern_type(*pb.element)?,
            },
            Type::ConstantBuffer(inner) => Node::ConstantBuffer {
                element: self.intern_type(*inner)?,
            },
            Type::Resource(r) => Node::Resource {
                element: r.element.map(|e| self.intern_type(*e)).transpose()?,
            },
            Type::SamplerState(_) | Type::SamplerComparisonState(_) => Node::Sampler,
            Type::Unknown(k, n) => return Err(anyhow!("unknown type: {k} {n}")),
            _ => Node::ScalarLike,
        };

        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        Ok(id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutKind {
    Struct,
    Array,
    ParameterBlock,
    Resource,
    Sampler,
    ConstantBuffer,
    ScalarLike, // XXX
}

#[derive(Clone, Debug)]
pub struct LayoutCursor {
    tree: Arc<ShaderTree>,
    node: NodeId,
    base: ShaderOffset,
}

impl LayoutCursor {
    fn from_subtree(tree: Arc<ShaderTree>, subtree: &SubTree) -> Self {
        Self {
            tree,
            node: subtree.node,
            base: subtree.base,
        }
    }

    fn apply_field(&self, edge: &FieldEdge) -> Self {
        Self {
            tree: Arc::clone(&self.tree),
            node: edge.child,
            base: ShaderOffset {
                bytes: self.base.bytes + edge.offset.bytes,
                set: self.base.set + edge.offset.set,
                binding_range: self.base.binding_range + edge.offset.binding_range,
                array_index: self.base.array_index,
                varying_input: self.base.varying_input + edge.offset.varying_input,
            },
        }
    }

    pub fn kind(&self) -> LayoutKind {
        match &self.tree.nodes[self.node.0] {
            Node::Struct { .. } => LayoutKind::Struct,
            Node::Array { .. } => LayoutKind::Array,
            Node::ParameterBlock { .. } => LayoutKind::ParameterBlock,
            Node::Resource { .. } => LayoutKind::Resource,
            Node::Sampler => LayoutKind::Sampler,
            Node::ConstantBuffer { .. } => LayoutKind::ConstantBuffer,
            Node::ScalarLike => LayoutKind::ScalarLike,
        }
    }

    pub fn offset(&self) -> ShaderOffset {
        self.base
    }

    pub fn element_layout(&self) -> Result<LayoutCursor> {
        let element = match &self.tree.nodes[self.node.0] {
            Node::Struct { .. } => None,
            Node::Array { element, .. } => Some(*element),
            Node::ParameterBlock { element, .. } => Some(*element),
            Node::Resource { element, .. } => *element,
            Node::Sampler => None,
            Node::ConstantBuffer { element } => Some(*element),
            Node::ScalarLike => None,
        };
        let Some(element) = element else {
            return Err(anyhow!("node does not have an element layout"));
        };
        Ok(Self {
            tree: self.tree.clone(),
            node: element,
            base: self.base,
        })
    }

    pub fn descriptor_set_layout(&self) -> Result<&DescriptorSetLayout> {
        match &self.tree.nodes[self.node.0] {
            Node::ParameterBlock { descriptor_set, .. } => Ok(descriptor_set),
            _ => Err(anyhow!("node is not a parameter block")),
        }
    }

    pub fn field(&self, name: &str) -> Result<Self> {
        let node = self.tree.node(self.node).context("node not found")?;
        let Node::Struct { fields } = &node else {
            return Err(anyhow!("not a struct layout"));
        };
        let edge = fields
            .iter()
            .find(|field| field.name == name)
            .context("field not found")?;
        Ok(self.apply_field(edge))
    }

    pub fn index(&self, index: usize) -> Result<Self> {
        let node = self.tree.node(self.node).context("node not found")?;
        let Node::Array {
            count,
            stride_bytes,
            stride_binding_range,
            stride_varying_input,
            element,
        } = &node
        else {
            return Err(anyhow!("not an array layout"));
        };
        let array_index = match count {
            ElementCount::Bounded(count) => {
                if index >= *count {
                    return Err(anyhow!("array layout index out-of-bounds"));
                }
                self.base.array_index * count + index
            }
            ElementCount::Runtime => self.base.array_index + index,
        };
        Ok(Self {
            tree: Arc::clone(&self.tree),
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
