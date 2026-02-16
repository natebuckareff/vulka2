#[cfg(debug_assertions)]
use std::{cell::Cell, rc::Rc};

pub struct LivenessToken {
    #[cfg(debug_assertions)]
    counter: Rc<Cell<usize>>,
}

#[cfg(debug_assertions)]
impl LivenessToken {
    pub(crate) fn new() -> Self {
        Self {
            counter: Rc::new(Cell::new(0)),
        }
    }

    pub(crate) fn guard(&self) -> LivenessGuard {
        self.counter.set(self.counter.get() + 1);
        LivenessGuard {
            counter: self.counter.clone(),
        }
    }
}

#[cfg(not(debug_assertions))]
impl LivenessToken {
    pub(crate) fn new() -> Self {
        Self {}
    }

    pub(crate) fn guard(&self) -> LivenessGuard {
        LivenessGuard {}
    }
}

#[cfg(debug_assertions)]
impl Drop for LivenessToken {
    fn drop(&mut self) {
        debug_assert_eq!(self.counter.get(), 0, "LivenessCounter is not zero");
    }
}

pub struct LivenessGuard {
    #[cfg(debug_assertions)]
    counter: Rc<Cell<usize>>,
}

#[cfg(debug_assertions)]
impl Drop for LivenessGuard {
    fn drop(&mut self) {
        self.counter.set(self.counter.get() - 1);
    }
}
