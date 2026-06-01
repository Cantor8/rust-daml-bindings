/// A ledger offset on the v2 Ledger API.
///
/// v2 replaced v1's tagged `LedgerOffset { absolute | boundary }` message
/// with a plain `int64` where:
///   - `0` denotes the ledger-begin sentinel (no transactions yet);
///   - positive values are absolute offsets.
///
/// v1's `End` boundary no longer exists as a sentinel — to consume up to
/// the current ledger end, fetch it with
/// `StateService::get_ledger_end` and pass the resulting offset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct DamlLedgerOffset(pub i64);

impl DamlLedgerOffset {
    /// The participant-begin sentinel: "start from the very first offset".
    pub const BEGIN: Self = Self(0);

    pub const fn new(offset: i64) -> Self {
        Self(offset)
    }

    pub const fn value(self) -> i64 {
        self.0
    }

    /// `true` when this offset is the ledger-begin sentinel.
    pub const fn is_begin(self) -> bool {
        self.0 == 0
    }
}

impl From<i64> for DamlLedgerOffset {
    fn from(v: i64) -> Self {
        Self(v)
    }
}

impl From<DamlLedgerOffset> for i64 {
    fn from(o: DamlLedgerOffset) -> Self {
        o.0
    }
}

/// Whether a stream should consume up to a fixed end offset or run
/// forever. Carried by request types (e.g. `UpdateService.GetUpdates`)
/// that need to distinguish "tail the ledger" from "give me a finite
/// historical slice".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DamlLedgerOffsetType {
    /// Open-ended: the stream keeps yielding new events as they land.
    Unbounded,
    /// Terminate after the named offset (inclusive on the wire).
    Bounded(DamlLedgerOffset),
}
