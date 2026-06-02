use crate::data::{DamlError, DamlResult};
use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use std::convert::TryFrom;
use std::time::Duration;

#[allow(clippy::cast_sign_loss)]
pub fn from_grpc_timestamp(timestamp: &prost_types::Timestamp) -> DateTime<Utc> {
    // `DateTime::from_timestamp` is None only for out-of-range
    // values; the proto allows the full chrono range, so this is
    // effectively infallible — fall back to the epoch on the
    // pathological case rather than introducing a Result return.
    DateTime::from_timestamp(timestamp.seconds, timestamp.nanos as u32)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).expect("epoch is in range"))
}

pub fn to_grpc_timestamp(datetime: DateTime<Utc>) -> DamlResult<prost_types::Timestamp> {
    Ok(prost_types::Timestamp {
        seconds: datetime.timestamp(),
        nanos: i32::try_from(datetime.nanosecond()).map_err(|e| DamlError::new_failed_conversion(e.to_string()))?,
    })
}

#[allow(clippy::cast_sign_loss)]
pub fn from_grpc_duration(duration: &prost_types::Duration) -> Duration {
    Duration::new(duration.seconds as u64, duration.nanos as u32)
}

pub fn to_grpc_duration(duration: &Duration) -> DamlResult<prost_types::Duration> {
    Ok(prost_types::Duration {
        seconds: i64::try_from(duration.as_secs()).map_err(|e| DamlError::new_failed_conversion(e.to_string()))?,
        nanos: i32::try_from(duration.subsec_nanos()).map_err(|e| DamlError::new_failed_conversion(e.to_string()))?,
    })
}

pub fn date_from_days(days: i32) -> DamlResult<NaiveDate> {
    NaiveDate::from_num_days_from_ce_opt(days.saturating_add(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap().num_days_from_ce()))
        .ok_or_else(|| DamlError::new_failed_conversion(format!("datetime from days {days} out of range")))
}

pub fn datetime_from_micros(micros: i64) -> DamlResult<DateTime<Utc>> {
    // micros since epoch -> DateTime<Utc>
    let secs = micros.div_euclid(1_000_000);
    let nanos_part = i64::rem_euclid(micros, 1_000_000) * 1_000;
    #[allow(clippy::cast_sign_loss)]
    DateTime::from_timestamp(secs, nanos_part as u32)
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
