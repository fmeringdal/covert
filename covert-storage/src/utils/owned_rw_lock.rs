use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};

/// A read write lock that gives ownership of the value during writes.
#[derive(Debug)]
pub struct OwnedRwLock<T>(RwLock<Option<T>>);

pub struct TransitionResult<T, U> {
    pub state: T,
    pub result: U,
}

impl<T> OwnedRwLock<T> {
    pub fn new(inner: T) -> Self {
        Self(RwLock::new(Some(inner)))
    }

    pub fn write<U, F>(&self, handler: F) -> U
    where
        F: FnOnce(T) -> TransitionResult<T, U>,
    {
        let mut inner_l = self.0.write();
        let inner = inner_l.take().expect("never none");
        let TransitionResult { state, result } = handler(inner);
        *inner_l = Some(state);
        result
    }

    pub fn read(&self) -> MappedRwLockReadGuard<'_, T> {
        let inner_l = self.0.read();
        RwLockReadGuard::map(inner_l, |inner| inner.as_ref().expect("never none"))
    }

    pub fn map<U, F: FnOnce(&T) -> U>(&self, handler: F) -> U {
        let inner_l = self.0.read();
        let inner = inner_l.as_ref().expect("never none");
        handler(inner)
    }
}
