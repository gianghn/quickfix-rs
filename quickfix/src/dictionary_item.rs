use crate::{DayOfWeek, Dictionary, QuickFixError};

/// Trait that represent quickfix configuration parameter.
///
/// Object that implement this trait can be apply to a dictionary.
///
/// # Dev note
///
/// Parameter key is constant, but value type can be different from one object to the other.
/// For example, `ConnectionType` is string, `SocketConnectPort` is integer, etc.
///
/// We cannot use generic in trait. So we need to "apply" inner value to the dictionary instead of
/// returning a generic value that the dictionary may use.
pub trait DictionaryItem {
    /// Apply inner value to dictionary.
    fn apply_param(&self, dict: &mut Dictionary) -> Result<(), QuickFixError>;
}

macro_rules! impl_dictionary_item {
    ($item:tt) => {
        impl DictionaryItem for $item {
            fn apply_param(&self, dict: &mut Dictionary) -> Result<(), QuickFixError> {
                dict.set(stringify!($item), self.0)
            }
        }
    };

    ($item:tt as String) => {
        impl DictionaryItem for $item<'_> {
            fn apply_param(&self, dict: &mut Dictionary) -> Result<(), QuickFixError> {
                dict.set(stringify!($item), self.0.to_string())
            }
        }
    };

    // QuickFIX reads these as a C int: a value it cannot hold is an error, not wrapped.
    ($item:ty as checked i32) => {
        impl DictionaryItem for $item {
            fn apply_param(&self, dict: &mut Dictionary) -> Result<(), QuickFixError> {
                let value = i32::try_from(self.0).map_err(|_| {
                    QuickFixError::invalid_argument(format!(
                        "{} {} is above {}",
                        stringify!($item),
                        self.0,
                        i32::MAX
                    ))
                })?;
                dict.set(stringify!($item), value)
            }
        }
    };

    ($item:ty as $target_type:ty) => {
        impl DictionaryItem for $item {
            fn apply_param(&self, dict: &mut Dictionary) -> Result<(), QuickFixError> {
                dict.set(stringify!($item), self.0 as $target_type)
            }
        }
    };
}

/// Session connection type.
#[derive(Debug)]
pub enum ConnectionType {
    /// Session is acceptor.
    Acceptor,

    /// Session is initiator.
    Initiator,
}

impl DictionaryItem for ConnectionType {
    fn apply_param(&self, dict: &mut Dictionary) -> Result<(), QuickFixError> {
        dict.set(
            "ConnectionType",
            match self {
                Self::Acceptor => "acceptor",
                Self::Initiator => "initiator",
            },
        )
    }
}

/// Acceptor port.
#[derive(Debug)]
pub struct SocketAcceptPort(pub u16);
impl_dictionary_item!(SocketAcceptPort as i32);

/// Initiator port to connect to.
#[derive(Debug)]
pub struct SocketConnectPort(pub u16);
impl_dictionary_item!(SocketConnectPort as i32);

/// Initiator host to connect to.
#[derive(Debug)]
pub struct SocketConnectHost<'a>(pub &'a str);
impl_dictionary_item!(SocketConnectHost as String);

/// Initiator port to connect from.
#[derive(Debug)]
pub struct SocketConnectSourcePort(pub u16);
impl_dictionary_item!(SocketConnectSourcePort as i32);

/// Initiator host to connect from.
#[derive(Debug)]
pub struct SocketConnectSourceHost<'a>(pub &'a str);
impl_dictionary_item!(SocketConnectSourceHost as String);

/// Set SO_REUSEADDR flag when creating socket.
#[derive(Debug)]
pub struct SocketReuseAddress(pub bool);
impl_dictionary_item!(SocketReuseAddress);

/// Set TCP_NODELAY flag when creating socket.
#[derive(Debug)]
pub struct SocketNodelay(pub bool);
impl_dictionary_item!(SocketNodelay);

/// Configure `SO_SNDBUF` socket parameter, in bytes (at most `i32::MAX`).
#[derive(Debug)]
pub struct SocketSendBufferSize(pub u32);
impl_dictionary_item!(SocketSendBufferSize as checked i32);

/// Configure `SO_RCVBUF` socket parameter, in bytes (at most `i32::MAX`).
#[derive(Debug)]
pub struct SocketReceiveBufferSize(pub u32);
impl_dictionary_item!(SocketReceiveBufferSize as checked i32);

/// Time in second to wait before reconnect.
#[derive(Debug)]
pub struct ReconnectInterval(pub u16);
impl_dictionary_item!(ReconnectInterval as i32);

/// FIX heartbeat time in seconds.
#[derive(Debug)]
pub struct HeartBtInt(pub u16);
impl_dictionary_item!(HeartBtInt as i32);

/// Send redundant resend requests.
#[derive(Debug)]
pub struct SendRedundantResendRequests(pub bool);
impl_dictionary_item!(SendRedundantResendRequests);

/// Send next expected message sequence number.
#[derive(Debug)]
pub struct SendNextExpectedMsgSeqNum(pub bool);
impl_dictionary_item!(SendNextExpectedMsgSeqNum);

/// Use local time.
#[derive(Debug)]
pub struct UseLocalTime(pub bool);
impl_dictionary_item!(UseLocalTime);

/// Session start time.
#[derive(Debug)]
pub struct StartTime<'a>(pub &'a str);
impl_dictionary_item!(StartTime as String);

/// Session start day.
#[derive(Debug)]
pub struct StartDay(pub DayOfWeek);
impl_dictionary_item!(StartDay);

/// Session end time.
#[derive(Debug)]
pub struct EndTime<'a>(pub &'a str);
impl_dictionary_item!(EndTime as String);

/// Session end day.
#[derive(Debug)]
pub struct EndDay(pub DayOfWeek);
impl_dictionary_item!(EndDay);

/// Logon time.
#[derive(Debug)]
pub struct LogonTime<'a>(pub &'a str);
impl_dictionary_item!(LogonTime as String);

/// Logon day.
#[derive(Debug)]
pub struct LogonDay(pub DayOfWeek);
impl_dictionary_item!(LogonDay);

/// Logon timeout.
#[derive(Debug)]
pub struct LogonTimeout(pub u16);
impl_dictionary_item!(LogonTimeout as i32);

/// Logout time.
#[derive(Debug)]
pub struct LogoutTime<'a>(pub &'a str);
impl_dictionary_item!(LogoutTime as String);

/// Logout day.
#[derive(Debug)]
pub struct LogoutDay(pub DayOfWeek);
impl_dictionary_item!(LogoutDay);

/// Logout timeout.
#[derive(Debug)]
pub struct LogoutTimeout(pub u16);
impl_dictionary_item!(LogoutTimeout as i32);

/// Application version ID.
#[derive(Debug)]
pub struct DefaultApplVerID<'a>(pub &'a str);
impl_dictionary_item!(DefaultApplVerID as String);

/// Parse and use data dictionary.
#[derive(Debug)]
pub struct UseDataDictionary(pub bool);
impl_dictionary_item!(UseDataDictionary);

/// Data dictionary XML spec path.
#[derive(Debug)]
pub struct DataDictionary<'a>(pub &'a str);
impl_dictionary_item!(DataDictionary as String);

/// Transport data dictionary XML spec path.
#[derive(Debug)]
pub struct TransportDataDictionary<'a>(pub &'a str);
impl_dictionary_item!(TransportDataDictionary as String);

/// Message store path.
#[derive(Debug)]
pub struct FileStorePath<'a>(pub &'a str);
impl_dictionary_item!(FileStorePath as String);

/// Validate message comp ID.
#[derive(Debug)]
pub struct CheckCompID(pub bool);
impl_dictionary_item!(CheckCompID);

/// Validate message latency.
#[derive(Debug)]
pub struct CheckLatency(pub bool);
impl_dictionary_item!(CheckLatency);

/// Max allowed latency.
#[derive(Debug)]
pub struct MaxLatency(pub i32);
impl_dictionary_item!(MaxLatency);

/// Validate message header length and checksum.
#[derive(Debug)]
pub struct ValidateLengthAndChecksum(pub bool);
impl_dictionary_item!(ValidateLengthAndChecksum);

/// Validate message fields sent in wrong order.
#[derive(Debug)]
pub struct ValidateFieldsOutOfOrder(pub bool);
impl_dictionary_item!(ValidateFieldsOutOfOrder);

/// Validate message fields inner values.
#[derive(Debug)]
pub struct ValidateFieldsHaveValues(pub bool);
impl_dictionary_item!(ValidateFieldsHaveValues);

/// Validate user custom fields.
#[derive(Debug)]
pub struct ValidateUserDefinedFields(pub bool);
impl_dictionary_item!(ValidateUserDefinedFields);

/// Allow unknown message fields.
#[derive(Debug)]
pub struct AllowUnknownMsgFields(pub bool);
impl_dictionary_item!(AllowUnknownMsgFields);

/// Preserve message fields order.
#[derive(Debug)]
pub struct PreserveMessageFieldsOrder(pub bool);
impl_dictionary_item!(PreserveMessageFieldsOrder);

/// Reset FIX session on logon.
#[derive(Debug)]
pub struct ResetOnLogon(pub bool);
impl_dictionary_item!(ResetOnLogon);

/// Reset FIX session on logout.
#[derive(Debug)]
pub struct ResetOnLogout(pub bool);
impl_dictionary_item!(ResetOnLogout);

/// Reset FIX session on disconnection.
#[derive(Debug)]
pub struct ResetOnDisconnect(pub bool);
impl_dictionary_item!(ResetOnDisconnect);

/// Refresh FIX session on logon.
#[derive(Debug)]
pub struct RefreshOnLogon(pub bool);
impl_dictionary_item!(RefreshOnLogon);

/// Built-in HTTP server main port.
#[derive(Debug)]
pub struct HttpAcceptPort(pub u16);
impl_dictionary_item!(HttpAcceptPort as i32);

/// Persist FIX messages.
#[derive(Debug)]
pub struct PersistMessages(pub bool);
impl_dictionary_item!(PersistMessages);

/// Reset sequence number as soon as session is initialized.
#[derive(Debug)]
pub struct SendResetSeqNumFlag(pub bool);
impl_dictionary_item!(SendResetSeqNumFlag);

/// Path where SSL certificate file can be found.
#[derive(Debug)]
pub struct ServerCertificateFile<'a>(pub &'a str);
impl_dictionary_item!(ServerCertificateFile as String);

/// Path where SSL certificate key file can be found.
#[derive(Debug)]
pub struct ServerCertificateKeyFile<'a>(pub &'a str);
impl_dictionary_item!(ServerCertificateKeyFile as String);

/// Path where SSL certificate file can be found.
#[derive(Debug)]
pub struct ClientCertificateFile<'a>(pub &'a str);
impl_dictionary_item!(ClientCertificateFile as String);

/// Path where SSL certificate key file can be found.
#[derive(Debug)]
pub struct ClientCertificateKeyFile<'a>(pub &'a str);
impl_dictionary_item!(ClientCertificateKeyFile as String);

/// Digits of sub-second precision in `SendingTime` (52) and the other UTC timestamps `QuickFIX`
/// writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampPrecision {
    /// `20261112-09:00:00`
    Seconds,
    /// `20261112-09:00:00.123`
    Milliseconds,
    /// `20261112-09:00:00.123456`
    Microseconds,
    /// `20261112-09:00:00.123456789`
    Nanoseconds,
}

impl DictionaryItem for TimestampPrecision {
    fn apply_param(&self, dict: &mut Dictionary) -> Result<(), QuickFixError> {
        let digits: i32 = match self {
            Self::Seconds => 0,
            Self::Milliseconds => 3,
            Self::Microseconds => 6,
            Self::Nanoseconds => 9,
        };
        dict.set("TimestampPrecision", digits)
    }
}

/// Enabled and active SSL protocol.
#[derive(Debug)]
pub enum SSLProtocol {
    /// This is the Secure Sockets Layer (SSL) protocol, version 2.0. It is the
    /// original SSL protocol as designed by Netscape Corporation.
    SSLv2,
    /// This is the Secure Sockets Layer (SSL) protocol, version 3.0. It is the
    /// successor to SSLv2 and the currently (as of February 1999) de-facto
    /// standardized SSL protocol from Netscape Corporation. It's supported by
    /// almost all popular browsers.
    SSLv3,
    /// This is the Transport Layer Security (TLS) protocol, version 1.0.
    TLSv1,
    /// This is the Transport Layer Security (TLS) protocol, version 1.1.
    TLSv1_1,
    /// This is the Transport Layer Security (TLS) protocol, version 1.2.
    TLSv1_2,
    /// This is a shortcut for `+SSLv2 +SSLv3 +TLSv1 +TLSv1_1 +TLSv1_2' and a convenient way for
    /// enabling all protocols except one when used in combination with the minus
    /// sign on a protocol as the example above shows.
    All,
}

impl DictionaryItem for SSLProtocol {
    fn apply_param(&self, dict: &mut Dictionary) -> Result<(), QuickFixError> {
        dict.set(
            "SSLProtocol",
            match self {
                Self::SSLv2 => "SSLv2",
                Self::SSLv3 => "SSLv3",
                Self::TLSv1 => "TLSv1",
                Self::TLSv1_1 => "TLSv1_1",
                Self::TLSv1_2 => "TLSv1_2",
                Self::All => "all",
            },
        )
    }
}

impl DictionaryItem for (&'static str, &str) {
    fn apply_param(&self, dict: &mut Dictionary) -> Result<(), QuickFixError> {
        dict.set(self.0, self.1.to_string())
    }
}

impl Dictionary {
    /// Build dictionary parameters from multiple items.
    pub fn try_from_items(items: &[&dyn DictionaryItem]) -> Result<Self, QuickFixError> {
        let mut output = Self::new();
        for item in items {
            item.apply_param(&mut output)?;
        }
        Ok(output)
    }
}
