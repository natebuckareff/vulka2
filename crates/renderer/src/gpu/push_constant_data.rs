use anyhow::{Context, Result, anyhow};
use bytemuck::Pod;
use slang::LayoutCursor;
use vulkanalia::vk;

pub struct PushConstantData {
    layout: LayoutCursor,
    range: vk::PushConstantRange,
    bytes: Vec<u8>,
}

impl PushConstantData {
    pub(crate) fn new(layout: LayoutCursor, range: vk::PushConstantRange) -> Result<Self> {
        let size = range
            .size
            .try_into()
            .context("push constant size exceeds usize")?;
        let bytes = vec![0; size];
        Ok(Self {
            layout,
            range,
            bytes,
        })
    }

    pub fn cursor(&mut self) -> PushConstantCursor<'_> {
        PushConstantCursor {
            layout: self.layout.clone(),
            bytes: &mut self.bytes,
        }
    }

    pub fn clear(&mut self) {
        self.bytes.fill(0);
    }

    pub(crate) fn range(&self) -> vk::PushConstantRange {
        self.range
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub struct PushConstantCursor<'data> {
    layout: LayoutCursor,
    bytes: &'data mut Vec<u8>,
}

impl<'data> PushConstantCursor<'data> {
    pub fn field(self, name: &str) -> Result<Self> {
        Ok(Self {
            layout: self.layout.field(name)?,
            bytes: self.bytes,
        })
    }

    pub fn index(self, index: usize) -> Result<Self> {
        Ok(Self {
            layout: self.layout.index(index)?,
            bytes: self.bytes,
        })
    }

    pub fn set<T: Pod>(self, value: T) -> Result<()> {
        self.write(&value)
    }

    pub fn write<T: Pod>(self, value: &T) -> Result<()> {
        let offset = self.layout.offset().bytes;
        let src = bytemuck::bytes_of(value);
        let end = offset
            .checked_add(src.len())
            .context("push constant write overflow")?;

        if end > self.bytes.len() {
            return Err(anyhow!(
                "push constant write out of bounds: {}..{} exceeds {}",
                offset,
                end,
                self.bytes.len()
            ));
        }

        self.bytes[offset..end].copy_from_slice(src);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use slang::{
        EntrypointLayout, LayoutSize, NumericType, PushConstantBufferType, PushConstantRangeLayout,
        ShaderLayout, ShaderTree, SlangShaderStage, Stride, StructType, Type, TypeLayout,
        VarLayout, VectorType,
    };
    use vulkanalia::vk;

    use crate::gpu::PushConstant;

    fn test_tree() -> Result<std::sync::Arc<ShaderTree>> {
        let vec4 = TypeLayout {
            size: Some(LayoutSize {
                bytes: Some(16),
                ..Default::default()
            }),
            alignment: 16,
            stride: Stride {
                bytes: 16,
                binding_range: 0,
            },
            ty: Type::Numeric(NumericType::Vector(VectorType {
                ty: "float".into(),
                count: 4,
            })),
        };

        let payload = TypeLayout {
            size: Some(LayoutSize {
                bytes: Some(32),
                ..Default::default()
            }),
            alignment: 16,
            stride: Stride {
                bytes: 32,
                binding_range: 0,
            },
            ty: Type::Struct(StructType {
                name: "PushData".into(),
                fields: vec![
                    VarLayout {
                        name: Some("a".into()),
                        offset_bytes: 0,
                        offset_set: 0,
                        offset_binding_range: 0,
                        varying: None,
                        value: vec4.clone(),
                    },
                    VarLayout {
                        name: Some("b".into()),
                        offset_bytes: 16,
                        offset_set: 0,
                        offset_binding_range: 0,
                        varying: None,
                        value: vec4,
                    },
                ],
            }),
        };

        let root = TypeLayout {
            size: Some(LayoutSize {
                push_constants: Some(1),
                bytes: Some(32),
                ..Default::default()
            }),
            alignment: 16,
            stride: Stride {
                bytes: 32,
                binding_range: 0,
            },
            ty: Type::PushConstantBuffer(PushConstantBufferType {
                layout: PushConstantRangeLayout {
                    stages: vk::ShaderStageFlags::VERTEX,
                    offset: 0,
                    size: 32,
                },
                element: Box::new(payload),
            }),
        };

        ShaderTree::new(ShaderLayout {
            bindless: None,
            globals: None,
            entrypoints: vec![EntrypointLayout {
                name: "main".into(),
                stage: SlangShaderStage::Vertex,
                params: Some(Box::new(VarLayout {
                    name: Some("main".into()),
                    offset_bytes: 0,
                    offset_set: 0,
                    offset_binding_range: 0,
                    varying: None,
                    value: root,
                })),
            }],
        })
    }

    #[test]
    fn push_constant_data_is_zero_initialized() -> Result<()> {
        let tree = test_tree()?;
        let layout = tree.entrypoint(SlangShaderStage::Vertex, "main")?;
        let push_constant = PushConstant::new(layout)?;
        let data = push_constant.data()?;

        assert_eq!(data.bytes(), &[0; 32]);
        Ok(())
    }

    #[test]
    fn push_constant_cursor_writes_field_bytes() -> Result<()> {
        let tree = test_tree()?;
        let layout = tree.entrypoint(SlangShaderStage::Vertex, "main")?;
        let push_constant = PushConstant::new(layout)?;
        let mut data = push_constant.data()?;

        data.cursor().field("b")?.set([5.0f32, 6.0, 7.0, 8.0])?;

        let mut expected = [0u8; 32];
        expected[16..32].copy_from_slice(bytemuck::bytes_of(&[5.0f32, 6.0, 7.0, 8.0]));
        assert_eq!(data.bytes(), &expected);
        Ok(())
    }

    #[test]
    fn push_constant_cursor_rejects_out_of_bounds_write() -> Result<()> {
        let payload = TypeLayout {
            size: Some(LayoutSize {
                bytes: Some(4),
                ..Default::default()
            }),
            alignment: 4,
            stride: Stride {
                bytes: 4,
                binding_range: 0,
            },
            ty: Type::Struct(StructType {
                name: "Tiny".into(),
                fields: vec![VarLayout {
                    name: Some("value".into()),
                    offset_bytes: 0,
                    offset_set: 0,
                    offset_binding_range: 0,
                    varying: None,
                    value: TypeLayout {
                        size: Some(LayoutSize {
                            bytes: Some(16),
                            ..Default::default()
                        }),
                        alignment: 16,
                        stride: Stride {
                            bytes: 16,
                            binding_range: 0,
                        },
                        ty: Type::Numeric(NumericType::Vector(VectorType {
                            ty: "float".into(),
                            count: 4,
                        })),
                    },
                }],
            }),
        };

        let tree = ShaderTree::new(ShaderLayout {
            bindless: None,
            globals: None,
            entrypoints: vec![EntrypointLayout {
                name: "main".into(),
                stage: SlangShaderStage::Vertex,
                params: Some(Box::new(VarLayout {
                    name: Some("main".into()),
                    offset_bytes: 0,
                    offset_set: 0,
                    offset_binding_range: 0,
                    varying: None,
                    value: TypeLayout {
                        size: Some(LayoutSize {
                            push_constants: Some(1),
                            bytes: Some(4),
                            ..Default::default()
                        }),
                        alignment: 4,
                        stride: Stride {
                            bytes: 4,
                            binding_range: 0,
                        },
                        ty: Type::PushConstantBuffer(PushConstantBufferType {
                            layout: PushConstantRangeLayout {
                                stages: vk::ShaderStageFlags::VERTEX,
                                offset: 0,
                                size: 4,
                            },
                            element: Box::new(payload),
                        }),
                    },
                })),
            }],
        })?;

        let layout = tree.entrypoint(SlangShaderStage::Vertex, "main")?;
        let push_constant = PushConstant::new(layout)?;
        let mut data = push_constant.data()?;

        let error = data
            .cursor()
            .field("value")?
            .write(&[1.0f32, 2.0, 3.0, 4.0])
            .expect_err("write should fail");

        assert!(error.to_string().contains("out of bounds"));
        Ok(())
    }
}
