use parking_lot::lock_api::ArcMutexGuard;
use parking_lot::{Mutex, RawMutex};
use std::sync::Arc;

struct SwitcherInner<T> {
    current: Mutex<Arc<Mutex<Option<T>>>>,
}

pub struct Switcher<T> {
    inner: Arc<SwitcherInner<T>>,
}

impl<T> Switcher<T> {
    pub fn new(value: T) -> Self {
        return Self {
            inner: Arc::new(SwitcherInner {
                current: Mutex::new(Arc::new(Mutex::new(Some(value)))),
            }),
        };
    }

    pub fn port(&mut self) -> (Port<T>, Control<T>) {
        let value = Arc::new(Mutex::new(None));

        let port = Port {
            value: value.clone(),
        };

        let control = Control {
            switcher: self.inner.clone(),
            port: value,
        };

        return (port, control);
    }
}

#[derive(Clone)]
pub struct Port<T> {
    value: Arc<Mutex<Option<T>>>,
}

impl<'s, T> Port<T> {
    pub fn access(&self) -> AccessGuard<T> {
        return self.value.lock_arc();
    }
}

pub type AccessGuard<T> = ArcMutexGuard<RawMutex, Option<T>>;

pub struct Control<T> {
    switcher: Arc<SwitcherInner<T>>,
    port: Arc<Mutex<Option<T>>>,
}

impl<T> Control<T> {
    pub fn switch(&self) {
        // Find currently active port
        let mut current = self.switcher.current.lock();

        // Acquire lock in active port
        let mut old = current.lock();

        // Remove value from active port
        let value = old.take().expect("Current port holds value");

        // Drop lock
        drop(old);

        // Acquire lock in new port
        let mut new = self.port.lock();

        // Set value to new port
        new.replace(value);

        // Drop Lock
        drop(new);

        // Update currently active port
        *current = self.port.clone();
    }

    pub fn is_active(&self) -> bool {
        let current = self.switcher.current.lock();
        return Arc::ptr_eq(&self.port, &*current);
    }
}
