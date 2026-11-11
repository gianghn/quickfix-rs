use std::{ffi::CString, fmt, mem::ManuallyDrop};

use quickfix_ffi::{
    FixMessage_addGroup, FixMessage_copy, FixMessage_copyGroup, FixMessage_copyHeader,
    FixMessage_copyTrailer, FixMessage_delete, FixMessage_fromString, FixMessage_getField,
    FixMessage_getFieldBytes, FixMessage_getGroupRef, FixMessage_getHeaderRef,
    FixMessage_getStringLen, FixMessage_getTrailerRef, FixMessage_new, FixMessage_readString,
    FixMessage_removeField, FixMessage_setField, FixMessage_t,
};

use crate::{
    group::Group,
    header::Header,
    trailer::Trailer,
    utils::{ffi_code_to_result, read_checked_cstr},
    view::{Borrowed, GroupMut, HeaderMut, TrailerMut},
    FieldMap, IntoFixValue, QuickFixError,
};

/// Base class for all FIX messages.
pub struct Message(pub(crate) FixMessage_t);

impl Message {
    /// Create new empty struct.
    pub fn new() -> Self {
        Self::default()
    }

    /// Try create new struct from raw text message.
    pub fn try_from_text(text: &str) -> Result<Self, QuickFixError> {
        let ffi_text = CString::new(text)?;
        unsafe { FixMessage_fromString(ffi_text.as_ptr()) }
            .map(Self)
            .ok_or_else(QuickFixError::from_last_error)
    }

    /// Return a raw FIX message pointer. The message keeps ownership: the pointer is valid only
    /// while `self` lives.
    pub fn get_fix_message_t(&self) -> FixMessage_t {
        self.0
    }

    /// Give up ownership of the underlying C++ message and return its raw pointer.
    ///
    /// The caller must hand it back exactly once to [`Message::from_fix_message_t`], or the
    /// message leaks.
    pub fn into_fix_message_t(self) -> FixMessage_t {
        ManuallyDrop::new(self).0
    }

    /// Take ownership of a raw FIX message pointer.
    ///
    /// Taking ownership of a pointer is not something safe code may do (R-P0-2):
    ///
    /// ```compile_fail
    /// # use quickfix::Message;
    /// let raw = Message::new().get_fix_message_t();
    /// let _adopted = Message::from_fix_message_t(raw);
    /// ```
    ///
    /// # Safety
    ///
    /// `fix_message_t` must point to a live C++ `FIX::Message` allocated by this library that
    /// nobody else owns: one returned by [`Message::into_fix_message_t`]. A pointer from
    /// [`Message::get_fix_message_t`], or a message borrowed in a callback, is still owned
    /// elsewhere and would be freed twice.
    ///
    /// # Errors
    ///
    /// None today; the `Result` is kept so callers do not change when validation is added.
    pub unsafe fn from_fix_message_t(fix_message_t: FixMessage_t) -> Result<Self, QuickFixError> {
        Ok(Self(fix_message_t))
    }

    /// Read a field's bytes exactly as stored, NULs and non-UTF-8 bytes included (R-P1-3).
    /// `get_field` stops at the first NUL; use this one for data fields (95/96) and audit.
    pub fn get_field_bytes(&self, tag: i32) -> Option<Vec<u8>> {
        let mut len = 0_u64;
        let ptr = unsafe { FixMessage_getFieldBytes(self.0, tag, &mut len) };
        if ptr.is_null() {
            return None;
        }
        let len = usize::try_from(len).ok()?;
        // SAFETY: the binding returned the field's buffer and its length; it lives in the message,
        // which `&self` keeps alive and unchanged while we copy.
        Some(unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) }.to_vec())
    }

    /// Try reading underlying struct buffer as a vector of bytes.
    ///
    /// # Performances
    ///
    /// Do not use this method in latency sensitive code.
    ///
    /// String will be generated twice in C++ code:
    /// - Once for getting a safe buffer length.
    /// - Then to copy buffer to rust "memory".
    pub fn to_fix_raw_bytes(&self) -> Result<Vec<u8>, QuickFixError> {
        unsafe {
            // Prepare output buffer
            let buffer_len = FixMessage_getStringLen(self.0)
                .try_into()
                .map_err(|_err| QuickFixError::from_last_error())?;

            // Allocate buffer on rust side
            let mut buffer = vec![0_u8; buffer_len as usize];
            assert_eq!(buffer.len(), buffer_len as usize);

            // Read text
            ffi_code_to_result(FixMessage_readString(
                self.0,
                buffer.as_mut_ptr().cast(),
                buffer_len,
            ))?;

            Ok(buffer)
        }
    }

    /// Read underlying struct buffer as raw FIX bytes, without the trailing NUL.
    ///
    /// Same as [`Self::to_fix_raw_bytes`] minus the C++ NUL terminator: no NUL scan, no UTF-8
    /// check, no second copy. For callers that keep the message as bytes (the KRX gateway).
    pub fn to_fix_bytes(&self) -> Result<Vec<u8>, QuickFixError> {
        let mut buffer = self.to_fix_raw_bytes()?;
        if buffer.last() == Some(&0) {
            buffer.pop();
        }
        Ok(buffer)
    }

    /// Try reading underlying struct buffer as a FIX string.
    ///
    /// # Performances
    ///
    /// Do not use this method in latency sensitive code.
    ///
    /// String will be generated twice in C++ code:
    /// - Once for getting a safe buffer length.
    /// - Then to copy buffer to rust "memory".
    pub fn to_fix_string(&self) -> Result<String, QuickFixError> {
        let buffer = self.to_fix_raw_bytes()?;

        // Convert to String
        //
        // NOTE: Here, I deliberately made the choice to drop C weird string / invalid UTF8 string
        //       content. If this happen, there is not so much we can do about ...
        //       Returning no error is sometime nicer, than an incomprehensible error.
        let text = CString::from_vec_with_nul(buffer).unwrap_or_default();
        Ok(text.to_string_lossy().into())
    }

    /// Clone struct header part.
    ///
    /// # Panic
    ///
    /// When memory allocation fail in C++ library.
    pub fn clone_header(&self) -> Header {
        unsafe { FixMessage_copyHeader(self.0) }
            .map(Header)
            .expect("Fail to allocate new Header")
    }

    /// Read struct header part.
    ///
    /// # Panic
    ///
    /// When struct pointer cannot be read from `FIX::Message`. This is
    /// something that could not be theoretically possible.
    pub fn with_header<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&Header) -> T,
    {
        let ptr =
            unsafe { FixMessage_getHeaderRef(self.0) }.expect("Fail to get ptr on message header");

        let obj = ManuallyDrop::new(Header(ptr));
        f(&obj)
    }

    /// Read or write struct header part.
    ///
    /// The header is borrowed from the message; it cannot be swapped out, which would free it
    /// under the message (R-P0-3):
    ///
    /// ```compile_fail
    /// # use quickfix::*;
    /// let mut msg = Message::new();
    /// msg.with_header_mut(|h| std::mem::swap(h, &mut Header::new()));
    /// ```
    ///
    /// # Panic
    ///
    /// When struct pointer cannot be read from `FIX::Message`. This is
    /// something that could not be theoretically possible.
    pub fn with_header_mut<T, F>(&mut self, f: F) -> T
    where
        F: FnOnce(&mut HeaderMut<'_>) -> T,
    {
        let ptr =
            unsafe { FixMessage_getHeaderRef(self.0) }.expect("Fail to get ptr on message header");

        f(&mut Borrowed::new(Header(ptr)))
    }

    /// Clone struct trailer part.
    ///
    /// # Panic
    ///
    /// When memory allocation fail in C++ library.
    pub fn clone_trailer(&self) -> Trailer {
        unsafe { FixMessage_copyTrailer(self.0) }
            .map(Trailer)
            .expect("Fail to allocate new Trailer")
    }

    /// Read struct trailer part.
    ///
    /// # Panic
    ///
    /// When struct pointer cannot be read from `FIX::Message`. This is
    /// something that could not be theoretically possible.
    pub fn with_trailer<T, F>(&self, f: F) -> T
    where
        F: FnOnce(&Trailer) -> T,
    {
        let ptr = unsafe { FixMessage_getTrailerRef(self.0) }
            .expect("Fail to get ptr on message trailer");

        let obj = ManuallyDrop::new(Trailer(ptr));
        f(&obj)
    }

    /// Read or write struct trailer part.
    ///
    /// ```compile_fail
    /// # use quickfix::*;
    /// let mut msg = Message::new();
    /// msg.with_trailer_mut(|t| std::mem::take(t));
    /// ```
    ///
    /// # Panic
    ///
    /// When struct pointer cannot be read from `FIX::Message`. This is
    /// something that could not be theoretically possible.
    pub fn with_trailer_mut<T, F>(&mut self, f: F) -> T
    where
        F: FnOnce(&mut TrailerMut<'_>) -> T,
    {
        let ptr = unsafe { FixMessage_getTrailerRef(self.0) }
            .expect("Fail to get ptr on message trailer");

        f(&mut Borrowed::new(Trailer(ptr)))
    }

    /// Read struct group part for a given tag and group index.
    pub fn with_group<T, F>(&self, index: i32, tag: i32, f: F) -> Option<T>
    where
        F: FnOnce(&GroupMut<'_>) -> T,
    {
        let ptr = unsafe { FixMessage_getGroupRef(self.0, index, tag) }?;
        Some(f(&GroupMut::new(Group(ptr), tag)))
    }

    /// Read or write struct group part for a given tag and group index.
    ///
    /// ```compile_fail
    /// # use quickfix::*;
    /// let mut msg = Message::new();
    /// let mut other = Group::try_new(33, 58).unwrap();
    /// msg.with_group_mut(1, 33, |g| std::mem::swap(g, &mut other));
    /// ```
    pub fn with_group_mut<T, F>(&mut self, index: i32, tag: i32, f: F) -> Option<T>
    where
        F: FnOnce(&mut GroupMut<'_>) -> T,
    {
        let ptr = unsafe { FixMessage_getGroupRef(self.0, index, tag) }?;
        Some(f(&mut GroupMut::new(Group(ptr), tag)))
    }
}

impl FieldMap for Message {
    fn get_field(&self, tag: i32) -> Option<String> {
        unsafe { FixMessage_getField(self.0, tag) }.map(read_checked_cstr)
    }

    fn set_field<V: IntoFixValue>(&mut self, tag: i32, value: V) -> Result<(), QuickFixError> {
        let fix_value = value.into_fix_value()?;
        ffi_code_to_result(unsafe { FixMessage_setField(self.0, tag, fix_value.as_ptr()) })
    }

    fn remove_field(&mut self, tag: i32) -> Result<(), QuickFixError> {
        ffi_code_to_result(unsafe { FixMessage_removeField(self.0, tag) })
    }

    fn add_group(&mut self, group: &Group) -> Result<(), QuickFixError> {
        ffi_code_to_result(unsafe { FixMessage_addGroup(self.0, group.0) })?;
        Ok(())
    }

    fn clone_group(&self, index: i32, tag: i32) -> Option<Group> {
        unsafe { FixMessage_copyGroup(self.0, index, tag) }.map(Group)
    }
}

impl Clone for Message {
    fn clone(&self) -> Self {
        Self(unsafe { FixMessage_copy(self.0) }.expect("Fail to clone Message"))
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut printer = f.debug_tuple("Message");

        if let Ok(txt) = self.to_fix_string() {
            printer.field(&txt.replace(1 as char, "|"));
        }

        printer.finish()
    }
}

impl Default for Message {
    fn default() -> Self {
        unsafe { FixMessage_new() }
            .map(Self)
            .expect("Fail to allocate new Message")
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        unsafe { FixMessage_delete(self.0) }
    }
}
