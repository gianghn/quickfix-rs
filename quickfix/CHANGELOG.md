# Changelog

## Unreleased (KRX fork, krx/main)

### Breaking changes

- A panic in any `ApplicationCallback` or `LogCallback` method now **aborts the process**. It used
  to be reported to QuickFIX as success, which lost the inbound message (its seqnum consumed) or
  sent the outbound one. Run the application under a supervisor that restarts it; the peer
  resends what the aborted process had not handled (R-P0-1).
- A callback result code the C++ binding does not know now aborts instead of being ignored.
- `ApplicationCallback` and `LogCallback` require `Send + Sync`: QuickFIX calls them from several
  threads (R-P0-4).
- `Message::from_fix_message_t` is `unsafe`; pair it with the new `Message::into_fix_message_t`
  (R-P0-2).
- `FixSocketServerKind::default()` is `MultiThreaded`; `SingleThreaded` is deprecated: QuickFIX's
  plain socket classes crash at random in `stop()` (R-P0-8).
- Borrowed parts are views, not `&mut` owned types (R-P0-3): `on_msg_to_admin` and
  `on_msg_to_app` take `&mut MessageMut<'_>`; `with_header_mut`, `with_trailer_mut`,
  `with_group_mut` and `with_group` give `HeaderMut`, `TrailerMut` and `GroupMut`. They read
  through `Deref` and write through `FieldMap`, and cannot be swapped out (which freed the C++
  object under its owner). `GroupMut::field_id()` is the tag it was looked up by; it has no
  `delim()`.
- `IntoFixValue::into_fix_value` returns `QuickFixError` (was `NulError`); `f32`/`f64` NaN and
  infinities are rejected instead of written as `44=NaN` (R-P0-10).
- Generated message crates: `TryFrom<quickfix::Message>` (error `FieldError`) replaces the
  `From` that asserted the MsgType; every field has a `try_get_*` returning `FieldError::Missing`
  or `FieldError::Invalid` instead of panicking; FIX booleans (`Y`/`N`) now parse (R-P0-9).

### Fixes

- Dropping an `Acceptor` or `Initiator` while another thread sends no longer frees the C++ session
  under that send: drop waits for in-flight sends, later sends get `SessionNotFound` (R-P0-6).

- `MsgFromAppError::UnsupportedMessageType` now reaches the peer as a BusinessMessageReject
  (35=j, 380=3); it used to be dropped without a reply (R-P0-5).
- Asking for an SSL acceptor or initiator in a build without SSL returns an error instead of
  terminating the process; every binding call also catches non-`std::exception` throws (R-P2-1).
- The binding's last-error message no longer leaks once per thread (R-P1-2).
- `clone_group` (message, header, trailer, group) and every generated group getter no longer read
  past the heap; the copied group reports its own field id and delimiter (R-P0-7).

## v0.2.2

### Breaking changes

Add support for multi threaded socket acceptor and initiator.

To create socket acceptor and initiator you must change below code:

```diff
- let mut acceptor = SocketInitiator::try_new(&settings, &app, &store_factory, &log_factory)?;
+ let mut acceptor = Initiator::try_new(
+     &settings,
+     &app,
+     &store_factory,
+     &log_factory,
+     FixSocketServerKind::default(),
+ )?;

- let mut initiator = SocketInitiator::try_new(&settings, &app, &message_store, &logger)?;
+ let mut initiator = Initiator::try_new(
+     &settings,
+     &app,
+     &message_store,
+     &logger,
+     FixSocketServerKind::default(),
+ )?;
```
