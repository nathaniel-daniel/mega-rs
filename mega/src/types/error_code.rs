/// An API Error
#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone, serde::Deserialize, serde::Serialize)]
pub struct ErrorCode(i32);

impl ErrorCode {
    /// Success
    pub const OK: Self = ErrorCode(0);

    /// Internal Error
    pub const EINTERNAL: Self = ErrorCode(-1);

    /// Invalid arguments
    pub const EARGS: Self = ErrorCode(-2);

    /// Invalid arguments
    pub const EAGAIN: Self = ErrorCode(-3);

    /// Ratelimited
    pub const ERATELIMIT: Self = ErrorCode(-4);

    /// Upload failed
    pub const EFAILED: Self = ErrorCode(-5);

    /// Too many ips are trying to access this resource
    pub const ETOOMANY: Self = ErrorCode(-6);

    /// The file packet is out of range
    pub const ERANGE: Self = ErrorCode(-7);

    /// The upload target url has expired
    pub const EEXPIRED: Self = ErrorCode(-8);

    /// Object not found
    pub const ENOENT: Self = ErrorCode(-9);

    /// Attempted circular link
    pub const ECIRCULAR: Self = ErrorCode(-10);

    /// Access violation (like writing to a read-only share)
    pub const EACCESS: Self = ErrorCode(-11);

    /// Tried to create an object that already exists
    pub const EEXIST: Self = ErrorCode(-12);

    /// Tried to access an incomplete resource
    pub const EINCOMPLETE: Self = ErrorCode(-13);

    /// A decryption operation failed
    pub const EKEY: Self = ErrorCode(-14);

    /// Invalid or expired user session
    pub const ESID: Self = ErrorCode(-15);

    /// User blocked
    pub const EBLOCKED: Self = ErrorCode(-16);

    /// Request over quota
    pub const EOVERQUOTA: Self = ErrorCode(-17);

    /// Resource temporarily unavailable
    pub const ETEMPUNAVAIL: Self = ErrorCode(-18);

    /// Too many connections to this resource
    pub const ETOOMANYCONNECTIONS: Self = ErrorCode(-19);

    /// Write failed
    pub const EWRITE: Self = ErrorCode(-20);

    /// Read failed
    pub const EREAD: Self = ErrorCode(-21);

    /// Invalid App key
    pub const EAPPKEY: Self = ErrorCode(-22);

    /// SSL verification failed
    pub const ESSL: Self = ErrorCode(-23);

    /// Not enough quota
    pub const EGOINGOVERQUOTA: Self = ErrorCode(-24);

    /// Need multifactor authentication
    pub const EMFAREQUIRED: Self = ErrorCode(-26);

    /// Access denied for sub-users (buisness accounts only)
    pub const EMASTERONLY: Self = ErrorCode(-27);

    /// Business account expired
    pub const EBUSINESSPASTDUE: Self = ErrorCode(-28);

    /// Over Disk Quota Paywall
    pub const EPAYWALL: Self = ErrorCode(-29);

    /// Get a human-friendly description if the error
    pub fn description(self) -> &'static str {
        match self {
            Self::OK => "No error",
            Self::EINTERNAL => "Internal error",
            Self::EARGS => "Invalid argument",
            Self::EAGAIN => "Request failed, retrying",
            Self::ERATELIMIT => "Rate limit exceeded",
            Self::EFAILED => "Failed permanently",
            Self::ETOOMANY => "Too many concurrent connections or transfers", // TODO: This can switch on a context variable
            Self::ERANGE => "Out of range",
            Self::EEXPIRED => "Expired",
            Self::ENOENT => "Not found",
            Self::ECIRCULAR => "Circular linkage detected", // TODO: This can switch on a context variable
            Self::EACCESS => "Access denied",
            Self::EEXIST => "Already exists",
            Self::EINCOMPLETE => "Incomplete",
            Self::EKEY => "Invalid key/Decryption error",
            Self::ESID => "Bad session ID",
            Self::EBLOCKED => "Blocked", // TODO: This can switch on a context variable
            Self::EOVERQUOTA => "Over quota",
            Self::ETEMPUNAVAIL => "Temporarily not available",
            Self::ETOOMANYCONNECTIONS => "Connection overflow",
            Self::EWRITE => "Write error",
            Self::EREAD => "Read error",
            Self::EAPPKEY => "Invalid application key",
            Self::ESSL => "SSL verification failed",
            Self::EGOINGOVERQUOTA => "Not enough quota",
            Self::EMFAREQUIRED => "Multi-factor authentication required",
            Self::EMASTERONLY => "Access denied for users",
            Self::EBUSINESSPASTDUE => "Business account has expired",
            Self::EPAYWALL => "Storage Quota Exceeded. Upgrade now",
            _ => "Unknown error",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl std::error::Error for ErrorCode {}
