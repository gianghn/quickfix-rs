//! Borrowed views over C++ objects that something else owns (R-P0-3).
//!
//! A `&mut Header` over a header owned by its message could be swapped out with `mem::swap` or
//! `mem::take`; dropping the escaped value then freed the header under the message. These views
//! have no `Drop`, `Default` or `Clone`, so nothing a caller can move out of them frees anything.

use std::{fmt, marker::PhantomData, mem::ManuallyDrop, ops::Deref};

use crate::{FieldMap, Group, Header, IntoFixValue, Message, QuickFixError, Trailer};

/// A FIX object borrowed from its owner: a message's header or trailer, or a message QuickFIX owns
/// and hands to a callback.
///
/// Reads go through `Deref` (`&T`); writes go through [`FieldMap`]. There is no `&mut T`.
pub struct Borrowed<'a, T>(ManuallyDrop<T>, PhantomData<&'a mut T>);

/// A message borrowed from QuickFIX in a callback.
pub type MessageMut<'a> = Borrowed<'a, Message>;
/// A header borrowed from its message.
pub type HeaderMut<'a> = Borrowed<'a, Header>;
/// A trailer borrowed from its message.
pub type TrailerMut<'a> = Borrowed<'a, Trailer>;

impl<T> Borrowed<'_, T> {
    /// `inner` must point to an object that outlives the view and is not freed through it.
    pub(crate) fn new(inner: T) -> Self {
        Self(ManuallyDrop::new(inner), PhantomData)
    }
}

impl<T> Deref for Borrowed<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: fmt::Debug> fmt::Debug for Borrowed<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Borrowed").field(&*self.0).finish()
    }
}

impl<T: FieldMap> FieldMap for Borrowed<'_, T> {
    fn get_field(&self, tag: i32) -> Option<String> {
        self.0.get_field(tag)
    }

    fn set_field<V: IntoFixValue>(&mut self, tag: i32, value: V) -> Result<(), QuickFixError> {
        self.0.set_field(tag, value)
    }

    fn remove_field(&mut self, tag: i32) -> Result<(), QuickFixError> {
        self.0.remove_field(tag)
    }

    fn add_group(&mut self, group: &Group) -> Result<(), QuickFixError> {
        self.0.add_group(group)
    }

    fn clone_group(&self, index: i32, tag: i32) -> Option<Group> {
        self.0.clone_group(index, tag)
    }
}

impl Borrowed<'_, Message> {
    /// Read or write the header of the borrowed message. See [`Message::with_header_mut`].
    pub fn with_header_mut<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut HeaderMut<'_>) -> R,
    {
        self.0.with_header_mut(f)
    }

    /// Read or write the trailer of the borrowed message. See [`Message::with_trailer_mut`].
    pub fn with_trailer_mut<R, F>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut TrailerMut<'_>) -> R,
    {
        self.0.with_trailer_mut(f)
    }

    /// Read or write a group entry of the borrowed message. See [`Message::with_group_mut`].
    pub fn with_group_mut<R, F>(&mut self, index: i32, tag: i32, f: F) -> Option<R>
    where
        F: FnOnce(&mut GroupMut<'_>) -> R,
    {
        self.0.with_group_mut(index, tag, f)
    }
}

/// A repeating-group entry borrowed from its message.
///
/// QuickFIX stores entries as plain field maps, not `Group`s, so this view has no delimiter and
/// reports the count tag it was looked up by (R-P0-7). Field access only touches the field map.
pub struct GroupMut<'a> {
    inner: ManuallyDrop<Group>,
    field_id: i32,
    _owner: PhantomData<&'a mut Group>,
}

impl GroupMut<'_> {
    /// `inner` must point to a group entry that outlives the view.
    pub(crate) fn new(inner: Group, field_id: i32) -> Self {
        Self {
            inner: ManuallyDrop::new(inner),
            field_id,
            _owner: PhantomData,
        }
    }

    /// The group's count tag (NoXXX), the tag it was looked up by.
    pub fn field_id(&self) -> i32 {
        self.field_id
    }
}

impl fmt::Debug for GroupMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupMut")
            .field("id", &self.field_id)
            .finish()
    }
}

impl FieldMap for GroupMut<'_> {
    fn get_field(&self, tag: i32) -> Option<String> {
        self.inner.get_field(tag)
    }

    fn set_field<V: IntoFixValue>(&mut self, tag: i32, value: V) -> Result<(), QuickFixError> {
        self.inner.set_field(tag, value)
    }

    fn remove_field(&mut self, tag: i32) -> Result<(), QuickFixError> {
        self.inner.remove_field(tag)
    }

    fn add_group(&mut self, group: &Group) -> Result<(), QuickFixError> {
        self.inner.add_group(group)
    }

    fn clone_group(&self, index: i32, tag: i32) -> Option<Group> {
        self.inner.clone_group(index, tag)
    }
}
