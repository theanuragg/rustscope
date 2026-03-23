use parking_lot::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::collectors::locks as inner;

/// A Mutex that records wait/hold times into the RustScope session.
pub struct ProfiledMutex<T> {
    id: inner::LockId,
    inner: Mutex<T>,
}

impl<T> ProfiledMutex<T> {
    pub fn new(value: T, name: &str) -> Self {
        let id = inner::register_lock(name);
        Self {
            id,
            inner: Mutex::new(value),
        }
    }

    pub fn lock(&self) -> ProfiledMutexGuard<'_, T> {
        let wait = inner::record_lock_wait_start(self.id);
        let guard = self.inner.lock();
        let hold = inner::record_lock_acquired(wait);
        ProfiledMutexGuard { guard, hold }
    }
}

pub struct ProfiledMutexGuard<'a, T> {
    guard: MutexGuard<'a, T>,
    hold: inner::HoldToken,
}

impl<'a, T> std::ops::Deref for ProfiledMutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a, T> std::ops::DerefMut for ProfiledMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<'a, T> Drop for ProfiledMutexGuard<'a, T> {
    fn drop(&mut self) {
        inner::record_lock_released(self.hold);
    }
}

/// An RwLock that records wait/hold times into the RustScope session.
pub struct ProfiledRwLock<T> {
    id: inner::LockId,
    inner: RwLock<T>,
}

impl<T> ProfiledRwLock<T> {
    pub fn new(value: T, name: &str) -> Self {
        let id = inner::register_lock(name);
        Self {
            id,
            inner: RwLock::new(value),
        }
    }

    pub fn read(&self) -> ProfiledRwLockReadGuard<'_, T> {
        let wait = inner::record_lock_wait_start(self.id);
        let guard = self.inner.read();
        let hold = inner::record_lock_acquired(wait);
        ProfiledRwLockReadGuard { guard, hold }
    }

    pub fn write(&self) -> ProfiledRwLockWriteGuard<'_, T> {
        let wait = inner::record_lock_wait_start(self.id);
        let guard = self.inner.write();
        let hold = inner::record_lock_acquired(wait);
        ProfiledRwLockWriteGuard { guard, hold }
    }
}

pub struct ProfiledRwLockReadGuard<'a, T> {
    guard: RwLockReadGuard<'a, T>,
    hold: inner::HoldToken,
}

impl<'a, T> std::ops::Deref for ProfiledRwLockReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a, T> Drop for ProfiledRwLockReadGuard<'a, T> {
    fn drop(&mut self) {
        inner::record_lock_released(self.hold);
    }
}

pub struct ProfiledRwLockWriteGuard<'a, T> {
    guard: RwLockWriteGuard<'a, T>,
    hold: inner::HoldToken,
}

impl<'a, T> std::ops::Deref for ProfiledRwLockWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a, T> std::ops::DerefMut for ProfiledRwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<'a, T> Drop for ProfiledRwLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        inner::record_lock_released(self.hold);
    }
}

