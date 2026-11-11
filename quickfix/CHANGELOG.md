# Changelog

## Unreleased (KRX fork, krx/main)

### Breaking changes

- A panic in any `ApplicationCallback` or `LogCallback` method now **aborts the process**. It used
  to be reported to QuickFIX as success, which lost the inbound message (its seqnum consumed) or
  sent the outbound one. Run the application under a supervisor that restarts it; the peer
  resends what the aborted process had not handled (R-P0-1).
- A callback result code the C++ binding does not know now aborts instead of being ignored.

### Fixes

- `MsgFromAppError::UnsupportedMessageType` now reaches the peer as a BusinessMessageReject
  (35=j, 380=3); it used to be dropped without a reply (R-P0-5).
- Asking for an SSL acceptor or initiator in a build without SSL returns an error instead of
  terminating the process; every binding call also catches non-`std::exception` throws (R-P2-1).
- The binding's last-error message no longer leaks once per thread (R-P1-2).

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
