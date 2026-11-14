//! Fixed, payload-free schema. Consumers serialize only these fields.

pub const SCHEMA_VERSION: u16 = 1;
pub const RECORD_CAPACITY: usize = 256;
pub const VIEW_CAPACITY: usize = 16;
pub const INVOCATION_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessRun(pub u128);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phase(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arm {
    pub run: ProcessRun,
    pub epoch: u64,
    pub view_generation: u64,
    pub phase: Phase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservationKey {
    pub arm: Arm,
    pub invocation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Action {
    Unknown,
    Copy,
    Cut,
    Paste,
    SelectAll,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ResponderOrigin {
    Text,
    Drawing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Decision {
    RefusedUnknownSelector,
    RefusedInactiveOrNoResponder,
    RefusedPolicyDisallowed,
    RefusedNoTextMode,
    RefusedNoTextFocus,
    AllowedExplicitPolicy,
    AllowedDefaultText,
    /// A stale or foreign menu-tagged delivery refused before any edit policy ran.
    RefusedMenuDelivery,
}
impl Decision {
    pub(crate) fn allowed(self) -> bool {
        matches!(self, Self::AllowedExplicitPolicy | Self::AllowedDefaultText)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Fallback {
    NotEligible,
    Invoked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Refused,
    Dispatched {
        key_down_handled: bool,
        fallback: Fallback,
    },
    Aborted,
    Invalidated,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Entry,
    Outcome(Outcome),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub schema_version: u16,
    pub key: ObservationKey,
    pub parent: Option<ObservationKey>,
    pub action: Action,
    pub responder: ResponderOrigin,
    pub decision: Option<Decision>,
    pub event: Event,
}
/// Bit positions are stable schema values; invalidity never clears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InvalidReason {
    UnknownSelector,
    Capacity,
    CounterOverflow,
    Contention,
    StaleIdentity,
    Outstanding,
    DisabledOutstanding,
    ViewLost,
    Aborted,
    InvalidSnapshot,
    Duplicate,
    InvalidOutcome,
    Cancelled,
}
impl InvalidReason {
    pub(crate) const fn bit(self) -> u64 {
        1 << self as u8
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObserverError {
    NotMainThread,
    NotStarted,
    AlreadyStarted,
    Disabled,
    InvalidRun,
    StaleIdentity,
    ViewUnavailable,
    Outstanding,
    Undrained,
    Capacity,
    CounterOverflow,
    Contention,
    InvalidSnapshot,
    InvalidAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub schema_version: u16,
    pub run: ProcessRun,
    pub latest_epoch: u64,
    pub enabled: bool,
    pub dropped_total: u64,
    pub invalid_reasons: u64,
    pub sticky_invalid: bool,
    pub outstanding: u16,
    pub reserved: u16,
    pub queued: u16,
    pub snapshot_sequence: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drain {
    /// Only the first `len` entries are present. No heap allocation is needed.
    pub records: [Option<Record>; RECORD_CAPACITY],
    pub len: usize,
    pub status: Status,
}
