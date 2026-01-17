use std::cell::OnceCell;

use vulkanalia::vk;

#[derive(Default, Clone)]
pub(crate) struct ExtensionNameArray {
    names: Vec<vk::ExtensionName>,
    ptrs: OnceCell<Vec<*const i8>>,
}

impl From<Vec<vk::ExtensionName>> for ExtensionNameArray {
    fn from(names: Vec<vk::ExtensionName>) -> Self {
        Self {
            names,
            ..Default::default()
        }
    }
}

impl From<Vec<&vk::ExtensionName>> for ExtensionNameArray {
    fn from(names: Vec<&vk::ExtensionName>) -> Self {
        Self {
            names: names.into_iter().copied().collect(),
            ..Default::default()
        }
    }
}

impl ExtensionNameArray {
    pub(crate) fn contains(&self, name: &vk::ExtensionName) -> bool {
        self.names.contains(name)
    }

    pub(crate) fn len(&self) -> usize {
        self.names.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub(crate) fn push(&mut self, name: vk::ExtensionName) {
        self.names.push(name);
        self.ptrs.take();
    }

    pub(crate) fn as_ptrs(&self) -> &[*const i8] {
        self.ptrs.get_or_init(|| {
            let mut ptrs = Vec::with_capacity(self.names.len());
            ptrs.extend(self.names.iter().map(|name| name.as_ptr()));
            ptrs
        })
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &vk::ExtensionName> {
        self.names.iter()
    }

    pub(crate) fn into_iter(self) -> impl Iterator<Item = vk::ExtensionName> {
        self.names.into_iter()
    }
}
