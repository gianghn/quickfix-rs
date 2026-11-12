use quickfix_ffi::{
    FixFileMessageStoreFactory_new, FixMemoryMessageStoreFactory_new,
    FixMessageStoreFactory_delete, FixMessageStoreFactory_t, FixNullMessageStoreFactory_new,
};

use crate::{QuickFixError, SessionSettings};

#[cfg(feature = "build-with-mysql")]
pub mod mysql;

#[cfg(feature = "build-with-postgres")]
pub mod postgres;

///  Object can be converted as a foreign object representing a `MessageStore`.
///
/// # Safety
///
/// `as_ffi_ptr` must return a pointer to a live `QuickFIX` `MessageStoreFactory` that stays valid
/// for as long as `self` lives: acceptors and initiators hand it to C++, which dereferences it.
/// Any other pointer is undefined behaviour, so implementing the trait is `unsafe` (R-P2-2):
///
/// ```compile_fail
/// use quickfix::FfiMessageStoreFactory;
/// use quickfix_ffi::FixMessageStoreFactory_t;
/// struct Bogus(FixMessageStoreFactory_t);
/// impl FfiMessageStoreFactory for Bogus {
///     fn as_ffi_ptr(&self) -> FixMessageStoreFactory_t { self.0 }
/// }
/// ```
pub unsafe trait FfiMessageStoreFactory {
    /// Get a representation of the message store as a FFI pointer.
    fn as_ffi_ptr(&self) -> FixMessageStoreFactory_t;
}

/// File based implementation of `MessageStore`.
#[derive(Debug)]
pub struct FileMessageStoreFactory(FixMessageStoreFactory_t);

impl FileMessageStoreFactory {
    /// Try to create new struct from settings.
    pub fn try_new(settings: &SessionSettings) -> Result<Self, QuickFixError> {
        unsafe { FixFileMessageStoreFactory_new(settings.0) }
            .map(Self)
            .ok_or_else(QuickFixError::from_last_error)
    }
}

// SAFETY: the pointer comes from `FixFileMessageStoreFactory_new` and is freed only by `drop`.
unsafe impl FfiMessageStoreFactory for FileMessageStoreFactory {
    fn as_ffi_ptr(&self) -> FixMessageStoreFactory_t {
        self.0
    }
}

impl Drop for FileMessageStoreFactory {
    fn drop(&mut self) {
        unsafe { FixMessageStoreFactory_delete(self.0) }
    }
}

/// In memory implementation of `MessageStore`.
#[derive(Debug)]
pub struct MemoryMessageStoreFactory(FixMessageStoreFactory_t);

impl MemoryMessageStoreFactory {
    /// Create new struct.
    pub fn new() -> Self {
        Self::default()
    }
}

// SAFETY: the pointer comes from `FixMemoryMessageStoreFactory_new` and is freed only by `drop`.
unsafe impl FfiMessageStoreFactory for MemoryMessageStoreFactory {
    fn as_ffi_ptr(&self) -> FixMessageStoreFactory_t {
        self.0
    }
}

impl Default for MemoryMessageStoreFactory {
    fn default() -> Self {
        unsafe { FixMemoryMessageStoreFactory_new() }
            .map(Self)
            .expect("Fail to allocate MemoryMessageStore")
    }
}

impl Drop for MemoryMessageStoreFactory {
    fn drop(&mut self) {
        unsafe { FixMessageStoreFactory_delete(self.0) }
    }
}

/// Null implementation of MessageStore.
///
/// Will not actually store messages. Useful for admin-only or market data-only applications.
#[derive(Debug)]
pub struct NullMessageStoreFactory(FixMessageStoreFactory_t);

impl NullMessageStoreFactory {
    /// Create new struct.
    pub fn new() -> Self {
        Self::default()
    }
}

// SAFETY: the pointer comes from `FixNullMessageStoreFactory_new` and is freed only by `drop`.
unsafe impl FfiMessageStoreFactory for NullMessageStoreFactory {
    fn as_ffi_ptr(&self) -> FixMessageStoreFactory_t {
        self.0
    }
}

impl Default for NullMessageStoreFactory {
    fn default() -> Self {
        unsafe { FixNullMessageStoreFactory_new() }
            .map(Self)
            .expect("Fail to allocate NullMessageStore")
    }
}

impl Drop for NullMessageStoreFactory {
    fn drop(&mut self) {
        unsafe { FixMessageStoreFactory_delete(self.0) }
    }
}
