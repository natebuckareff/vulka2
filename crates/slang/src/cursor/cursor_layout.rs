use anyhow::{Result, anyhow};
use compact_str::CompactString;

use crate::{
    DescriptorSet, ElementCount, ShaderLayout, SlangShaderStage, Type, TypeLayout, VarLayout,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct NodeId(pub usize);

#[derive(Clone, Copy, Debug, Default)]
pub struct BaseOffset {
    pub bytes: usize,
    pub set: usize,
    pub binding_range: i64,
    pub array_index: usize,
}

#[derive(Debug)]
pub struct Root {
    pub node: NodeId,
    pub base: BaseOffset,
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
    pub offset_binding_range: i64,
    pub child: NodeId,
}

#[derive(Debug)]
pub enum NodeKind<'a> {
    Struct {
        fields: Vec<FieldEdge>,
    },
    Array {
        count: ElementCount,
        stride_bytes: usize,
        stride_binding_range: i64,
        element: NodeId,
    },
    ParameterBlock {
        descriptor_set: &'a DescriptorSet,
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
pub struct Node<'a> {
    pub ty: &'a TypeLayout,
    pub kind: NodeKind<'a>,
}

#[derive(Debug)]
pub struct CursorLayout<'a> {
    pub nodes: Vec<Node<'a>>,
    pub global: Option<Root>,
    pub entrypoints: Vec<EntrypointRoot>,
}

impl<'a> CursorLayout<'a> {
    pub fn build(layout: &'a ShaderLayout) -> Result<CursorLayout<'a>> {
        LayoutIndexer::build(layout)
    }
}

pub struct LayoutIndexer<'a> {
    nodes: Vec<Node<'a>>,
}

impl<'a> LayoutIndexer<'a> {
    fn build(layout: &'a ShaderLayout) -> Result<CursorLayout<'a>> {
        let mut b = Self { nodes: Vec::new() };

        let global = layout
            .globals
            .as_deref()
            .map(|v| b.root_from_var(v))
            .transpose()?;

        let mut entrypoints = Vec::with_capacity(layout.entrypoints.len());
        for ep in &layout.entrypoints {
            if let Some(params) = ep.params.as_deref() {
                entrypoints.push(EntrypointRoot {
                    name: ep.name.clone(),
                    stage: ep.stage,
                    root: b.root_from_var(params)?,
                });
            }
        }

        Ok(CursorLayout {
            nodes: b.nodes,
            global,
            entrypoints,
        })
    }

    fn root_from_var(&mut self, var: &'a VarLayout) -> Result<Root> {
        Ok(Root {
            node: self.intern_type(&var.value)?,
            base: BaseOffset {
                bytes: var.offset_bytes,
                set: var.offset_set,
                binding_range: var.offset_binding_range,
                array_index: 0,
            },
        })
    }

    fn intern_type(&mut self, ty: &'a TypeLayout) -> Result<NodeId> {
        let kind = match &ty.ty {
            Type::Struct(s) => {
                let mut fields = Vec::with_capacity(s.fields.len());
                for f in &s.fields {
                    fields.push(FieldEdge {
                        name: f.name.clone().unwrap_or_default(),
                        offset_bytes: f.offset_bytes,
                        offset_binding_range: f.offset_binding_range,
                        child: self.intern_type(&f.value)?,
                    });
                }
                NodeKind::Struct { fields }
            }
            Type::Array(a) => NodeKind::Array {
                count: a.count.clone(),
                stride_bytes: ty.stride.bytes,
                stride_binding_range: ty.stride.binding_range,
                element: self.intern_type(&a.element)?,
            },
            Type::ParameterBlock(pb) => NodeKind::ParameterBlock {
                descriptor_set: &pb.descriptor_set,
                element: self.intern_type(&pb.element)?,
            },
            Type::ConstantBuffer(inner) => NodeKind::ConstantBuffer {
                element: self.intern_type(inner)?,
            },
            Type::Resource(r) => NodeKind::Resource {
                element: r
                    .element
                    .as_deref()
                    .map(|e| self.intern_type(e))
                    .transpose()?,
            },
            Type::Unknown(k, n) => return Err(anyhow!("unknown type: {k} {n}")),
            _ => NodeKind::ScalarLike,
        };

        let id = NodeId(self.nodes.len());
        self.nodes.push(Node { ty, kind });
        Ok(id)
    }
}
