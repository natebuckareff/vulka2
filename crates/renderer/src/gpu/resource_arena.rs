// TODO: this is not really being used in a coherent way currently; need to
// re-evaluate this

use std::any::Any;
use std::cell::{RefCell, UnsafeCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use generational_arena::{Arena, Index};

struct DropCell<T> {
    set: AtomicBool,
    value: UnsafeCell<Option<T>>,
}

unsafe impl<T: Send> Send for DropCell<T> {}
unsafe impl<T: Send> Sync for DropCell<T> {}

impl<T> DropCell<T> {
    fn new() -> Self {
        Self {
            set: AtomicBool::new(false),
            value: UnsafeCell::new(None),
        }
    }

    fn set(&self, v: T) -> std::result::Result<(), T> {
        if self.set.swap(true, Ordering::AcqRel) {
            return Err(v);
        }
        unsafe { *self.value.get() = Some(v) };
        Ok(())
    }
}

impl<T> Drop for DropCell<T> {
    fn drop(&mut self) {
        // Safe: drop only runs once the last Arc is gone.
        unsafe {
            let _ = (*self.value.get()).take();
        }
    }
}

pub trait VulkanResource: Any + Send + 'static {
    type Raw: Clone;
    type Handle = VulkanHandle<Self::Raw>;
    unsafe fn raw(&self) -> &Self::Raw;
}

trait ErasedResource: Any + Send {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any + Send> ErasedResource for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub struct VulkanHandle<T: Clone> {
    cell: Arc<DropCell<ResourceArenaInner>>,
    handle: T,
}

impl<T: Clone> VulkanHandle<T> {
    fn new(cell: Arc<DropCell<ResourceArenaInner>>, handle: T) -> Self {
        Self { cell, handle }
    }

    pub unsafe fn raw(&self) -> &T {
        &self.handle
    }
}

impl<T: Clone> Clone for VulkanHandle<T> {
    fn clone(&self) -> Self {
        Self {
            cell: self.cell.clone(),
            handle: self.handle.clone(),
        }
    }
}

struct ResourceArenaInner {
    resources: Arena<Box<dyn ErasedResource>>,
    destruct: Vec<Index>,
}

impl ResourceArenaInner {
    fn new() -> Self {
        Self {
            resources: Arena::new(),
            destruct: Vec::new(),
        }
    }
}

impl Drop for ResourceArenaInner {
    fn drop(&mut self) {
        for index in self.destruct.drain(..).rev() {
            let _ = self.resources.remove(index);
        }
    }
}

pub struct ResourceArena {
    _label: &'static str,
    cell: RefCell<Arc<DropCell<ResourceArenaInner>>>,
    inner: RefCell<ResourceArenaInner>,
}

impl ResourceArena {
    pub fn new(label: &'static str) -> Self {
        let arena = Self {
            _label: label,
            cell: RefCell::new(Arc::new(DropCell::new())),
            inner: RefCell::new(ResourceArenaInner::new()),
        };
        arena
    }

    pub fn add<T: VulkanResource>(&self, resource: T) -> Result<VulkanHandle<T::Raw>> {
        let mut inner = self.inner.try_borrow_mut()?;
        let handle = unsafe { resource.raw().clone() };
        let index = inner.resources.insert(Box::new(resource));
        inner.destruct.push(index);
        let cell = self.cell.try_borrow()?;
        Ok(VulkanHandle::new(cell.clone(), handle))
    }

    pub fn clear(&self) -> Result<()> {
        let mut inner = self.inner.try_borrow_mut()?;
        let mut cell = self.cell.try_borrow_mut()?;

        let new_inner = ResourceArenaInner::new();
        let old_inner = std::mem::replace(&mut *inner, new_inner);
        drop(inner);

        let new_cell = Arc::new(DropCell::new());
        let old_cell = std::mem::replace(&mut *cell, new_cell);
        drop(cell);

        let result = old_cell.set(old_inner);
        debug_assert!(result.is_ok(), "cell should never be set twice");

        Ok(())
    }
}

impl Drop for ResourceArena {
    fn drop(&mut self) {
        let old_inner = self.inner.get_mut();
        let empty_inner = std::mem::replace(&mut *old_inner, ResourceArenaInner::new());
        let result = self.cell.get_mut().set(empty_inner);
        debug_assert!(result.is_ok(), "cell should never be set twice");
    }
}
