use crate::data::{DamlError, DamlResult};
use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use std::convert::TryFrom;
use std::time::Duration;

/// Decode a `google.protobuf.Timestamp` to a `chrono::DateTime<Utc>`.
///
/// Returns `FailedConversion` when the proto carries a negative nanos
/// field (the proto's `nanos: i32` is documented as `0..=999_999_999`)
/// or when the resulting `(seconds, nanos)` pair falls outside the
/// representable `DateTime` range.
pub fn from_grpc_timestamp(timestamp: &prost_types::Timestamp) -> DamlResult<DateTime<Utc>> {
    let nanos = u32::try_from(timestamp.nanos)
        .map_err(|_| DamlError::new_failed_conversion(format!("negative nanos {}", timestamp.nanos)))?;
    DateTime::from_timestamp(timestamp.seconds, nanos).ok_or_else(|| {
        DamlError::new_failed_conversion(format!("timestamp ({}, {}) out of range", timestamp.seconds, nanos))
    })
}

pub fn to_grpc_timestamp(datetime: DateTime<Utc>) -> DamlResult<prost_types::Timestamp> {
    Ok(prost_types::Timestamp {
        seconds: datetime.timestamp(),
        nanos: i32::try_from(datetime.nanosecond()).map_err(|e| DamlError::new_failed_conversion(e.to_string()))?,
    })
}

/// Decode a `google.protobuf.Duration` to a `std::time::Duration`.
///
/// Returns `FailedConversion` when either component is negative (the
/// proto allows signed values for "negative duration" semantics, but
/// `std::time::Duration` is unsigned and cannot represent that).
pub fn from_grpc_duration(duration: &prost_types::Duration) -> DamlResult<Duration> {
    let seconds = u64::try_from(duration.seconds)
        .map_err(|_| DamlError::new_failed_conversion(format!("negative seconds {}", duration.seconds)))?;
    let nanos = u32::try_from(duration.nanos)
        .map_err(|_| DamlError::new_failed_conversion(format!("negative nanos {}", duration.nanos)))?;
    Ok(Duration::new(seconds, nanos))
}

pub fn to_grpc_duration(duration: &Duration) -> DamlResult<prost_types::Duration> {
    Ok(prost_types::Duration {
        seconds: i64::try_from(duration.as_secs()).map_err(|e| DamlError::new_failed_conversion(e.to_string()))?,
        nanos: i32::try_from(duration.subsec_nanos()).map_err(|e| DamlError::new_failed_conversion(e.to_string()))?,
    })
}

pub fn date_from_days(days: i32) -> DamlResult<NaiveDate> {
    let epoch_ce = NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch is a valid date").num_days_from_ce();
    let offset = days
        .checked_add(epoch_ce)
        .ok_or_else(|| DamlError::new_failed_conversion(format!("date from days {days} overflowed i32")))?;
    NaiveDate::from_num_days_from_ce_opt(offset)
        .ok_or_else(|| DamlError::new_failed_conversion(format!("date from days {days} out of range")))
}

pub fn datetime_from_micros(micros: i64) -> DamlResult<DateTime<Utc>> {
    // micros since epoch -> DateTime<Utc>
    let secs = micros.div_euclid(1_000_000);
    // rem_euclid(1e6) < 1e6, then * 1e3 gives < 1e9 — always fits in u32.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let nanos_part = (i64::rem_euclid(micros, 1_000_000) * 1_000) as u32;
    DateTime::from_timestamp(secs, nanos_part)
        .ok_or_else(|| DamlError::new_failed_conversion(format!("datetime from micros {micros} out of range")))
}

#[allow(clippy::cast_possible_truncation)]
pub fn days_from_date(date: NaiveDate) -> i32 {
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch is a valid date");
    date.signed_duration_since(epoch).num_days() as i32
}

/// Required value.
pub trait Required<T> {
    fn req(self) -> DamlResult<T>;
}

impl<T> Required<T> for Option<T> {
    fn req(self) -> DamlResult<T> {
        self.ok_or(DamlError::MissingRequiredField)
    }
}
