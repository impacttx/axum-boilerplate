use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Deserializer};

/// Parse a string loosely as a `NaiveDate`.
///
/// Supports:
/// - Plain date: `YYYY-MM-DD` or `YYYY/MM/DD`
/// - DateTime without timezone: `YYYY-MM-DDTHH:MM:SS`, `YYYY-MM-DD HH:MM:SS` (with optional fractional seconds)
/// - RFC 3339 / ISO 8601 with timezone: `YYYY-MM-DDTHH:MM:SSZ`, `YYYY-MM-DDTHH:MM:SS+00:00`, etc.
/// - Any string starting with `YYYY-MM-DD[T or space]`
pub fn parse_date(input: &str) -> Result<NaiveDate, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("date string cannot be empty".to_string());
    }

    // 1. Try standard YYYY-MM-DD
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(d);
    }

    // 2. Try RFC 3339 (e.g. 2026-10-01T00:00:00Z, 2026-10-01T00:00:00+00:00)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.date_naive());
    }

    // 3. Try NaiveDateTime without timezone (e.g. 2026-10-01T00:00:00)
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(ndt.date());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(ndt.date());
    }

    // 4. Try space-separated datetime (e.g. 2026-10-01 00:00:00)
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(ndt.date());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(ndt.date());
    }

    // 5. Try prefix YYYY-MM-DD if followed by 'T' or space
    if s.len() > 10
        && s.is_char_boundary(10)
        && let Some(&b) = s.as_bytes().get(10)
        && (b == b'T' || b == b' ')
        && let Ok(d) = NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d")
    {
        return Ok(d);
    }

    // 6. Try alternative slash separator YYYY/MM/DD
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y/%m/%d") {
        return Ok(d);
    }
    if s.len() > 10
        && s.is_char_boundary(10)
        && let Some(&b) = s.as_bytes().get(10)
        && (b == b'T' || b == b' ')
        && let Ok(d) = NaiveDate::parse_from_str(&s[..10], "%Y/%m/%d")
    {
        return Ok(d);
    }

    Err(format!("invalid date format: '{s}'. Expected formats like 'YYYY-MM-DD' or 'YYYY-MM-DDTHH:MM:SS'"))
}

/// Parse a string loosely as a `DateTime<FixedOffset>`.
///
/// Supports:
/// - RFC 3339 / ISO 8601 with timezone (e.g. `2026-10-01T00:00:00Z`, `2026-10-01T00:00:00+00:00`)
/// - DateTime without timezone (assumed UTC): `YYYY-MM-DDTHH:MM:SS`, `YYYY-MM-DD HH:MM:SS`
/// - Date only (assumed midnight UTC): `YYYY-MM-DD`
pub fn parse_datetime(input: &str) -> Result<DateTime<FixedOffset>, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("datetime string cannot be empty".to_string());
    }

    // 1. Try RFC 3339
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt);
    }

    // 2. Try NaiveDateTime without timezone (assume UTC)
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&ndt).fixed_offset());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(Utc.from_utc_datetime(&ndt).fixed_offset());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&ndt).fixed_offset());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(Utc.from_utc_datetime(&ndt).fixed_offset());
    }

    // 3. Try date only (assume midnight UTC)
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        && let Some(ndt) = d.and_hms_opt(0, 0, 0)
    {
        return Ok(Utc.from_utc_datetime(&ndt).fixed_offset());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y/%m/%d")
        && let Some(ndt) = d.and_hms_opt(0, 0, 0)
    {
        return Ok(Utc.from_utc_datetime(&ndt).fixed_offset());
    }

    Err(format!(
        "invalid datetime format: '{s}'. Expected RFC 3339 (e.g. '2026-10-01T00:00:00Z') or 'YYYY-MM-DDTHH:MM:SS' or 'YYYY-MM-DD'"
    ))
}

/// Parse a string loosely as an end-of-day `DateTime<FixedOffset>` when only a date is given.
///
/// If a date string like `YYYY-MM-DD` or `YYYY/MM/DD` is given, returns that date at 23:59:59.999999999 UTC.
/// If time is already specified, preserves that time.
pub fn parse_datetime_end_of_day(input: &str) -> Result<DateTime<FixedOffset>, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("datetime string cannot be empty".to_string());
    }

    // If pure date format (YYYY-MM-DD or YYYY/MM/DD), set to end of day
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        && let Some(ndt) = d.and_hms_nano_opt(23, 59, 59, 999_999_999)
    {
        return Ok(Utc.from_utc_datetime(&ndt).fixed_offset());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y/%m/%d")
        && let Some(ndt) = d.and_hms_nano_opt(23, 59, 59, 999_999_999)
    {
        return Ok(Utc.from_utc_datetime(&ndt).fixed_offset());
    }

    parse_datetime(s)
}

/// Parse a string loosely as a `NaiveTime`.
///
/// Supports:
/// - `HH:MM:SS`
/// - `HH:MM`
/// - `HH:MM:SS.fff`
/// - `YYYY-MM-DDTHH:MM` / `YYYY-MM-DDTHH:MM:SS` (extracts time component)
/// - `hh:mm AM/PM`
pub fn parse_time(input: &str) -> Result<NaiveTime, String> {
    let mut s = input.trim();
    if s.is_empty() {
        return Err("time string cannot be empty".to_string());
    }

    // If input contains date-time separator 'T' (e.g. "2026-10-26T08:00" or "2026-10-26T08:00:00Z"),
    // extract the time part after 'T'.
    if let Some((_, time_part)) = s.split_once('T') {
        s = time_part.trim();
    }

    // Strip trailing 'Z' or 'z' if present
    let s = s.trim_end_matches(['Z', 'z']).trim();

    // Strip timezone offset if present (e.g. "+00:00", "-04:00")
    let s = if let Some(idx) = s.rfind(['+', '-']) {
        if idx > 0 && s[..idx].contains(':') { s[..idx].trim() } else { s }
    } else {
        s
    };

    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M:%S") {
        return Ok(t);
    }
    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M") {
        return Ok(t);
    }
    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M:%S%.f") {
        return Ok(t);
    }
    if let Ok(t) = NaiveTime::parse_from_str(s, "%I:%M %p") {
        return Ok(t);
    }
    if let Ok(t) = NaiveTime::parse_from_str(s, "%I:%M:%S %p") {
        return Ok(t);
    }

    Err(format!("invalid time format: '{input}'. Expected 'HH:MM' or 'HH:MM:SS'"))
}

/// Serde deserializer for `Option<NaiveDate>`.
///
/// Empty strings or whitespace are deserialized as `None`.
pub fn deserialize_opt_date<'de, D>(deserializer: D) -> Result<Option<NaiveDate>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            let s = s.trim();
            if s.is_empty() { Ok(None) } else { parse_date(s).map(Some).map_err(serde::de::Error::custom) }
        }
        None => Ok(None),
    }
}

/// Serde deserializer for `NaiveDate`.
pub fn deserialize_date<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_date(s.trim()).map_err(serde::de::Error::custom)
}

/// Serde deserializer for `Option<DateTime<FixedOffset>>`.
///
/// Empty strings or whitespace are deserialized as `None`.
pub fn deserialize_opt_datetime<'de, D>(deserializer: D) -> Result<Option<DateTime<FixedOffset>>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            let s = s.trim();
            if s.is_empty() { Ok(None) } else { parse_datetime(s).map(Some).map_err(serde::de::Error::custom) }
        }
        None => Ok(None),
    }
}

/// Serde deserializer for `Option<DateTime<FixedOffset>>` upper-bound queries (e.g. `date_to`).
///
/// If input is a pure date `YYYY-MM-DD`, sets time to 23:59:59.999999999 UTC.
/// Empty strings or whitespace are deserialized as `None`.
pub fn deserialize_opt_datetime_end_of_day<'de, D>(deserializer: D) -> Result<Option<DateTime<FixedOffset>>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            let s = s.trim();
            if s.is_empty() {
                Ok(None)
            } else {
                parse_datetime_end_of_day(s).map(Some).map_err(serde::de::Error::custom)
            }
        }
        None => Ok(None),
    }
}

/// Serde deserializer for `DateTime<FixedOffset>`.
pub fn deserialize_datetime<'de, D>(deserializer: D) -> Result<DateTime<FixedOffset>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_datetime(s.trim()).map_err(serde::de::Error::custom)
}

/// Serde deserializer for `Option<DateTime<Utc>>`.
///
/// Empty strings or whitespace are deserialized as `None`.
pub fn deserialize_opt_datetime_utc<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            let s = s.trim();
            if s.is_empty() {
                Ok(None)
            } else {
                parse_datetime(s).map(|dt| dt.to_utc()).map(Some).map_err(serde::de::Error::custom)
            }
        }
        None => Ok(None),
    }
}

/// Serde deserializer for `DateTime<Utc>`.
pub fn deserialize_datetime_utc<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_datetime(s.trim()).map(|dt| dt.to_utc()).map_err(serde::de::Error::custom)
}

/// Serde deserializer for `Option<NaiveTime>`.
///
/// Empty strings or whitespace are deserialized as `None`.
pub fn deserialize_opt_time<'de, D>(deserializer: D) -> Result<Option<NaiveTime>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            let s = s.trim();
            if s.is_empty() { Ok(None) } else { parse_time(s).map(Some).map_err(serde::de::Error::custom) }
        }
        None => Ok(None),
    }
}

/// Serde deserializer for `NaiveTime`.
pub fn deserialize_time<'de, D>(deserializer: D) -> Result<NaiveTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_time(s.trim()).map_err(serde::de::Error::custom)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BoolHelper {
    Bool(bool),
    Str(String),
}

/// Serde deserializer for `Option<bool>` supporting strings like `"true"`, `"1"`, `""` (flag with no value), `"false"`, `"0"`.
pub fn deserialize_opt_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<BoolHelper>::deserialize(deserializer)?;
    match opt {
        Some(BoolHelper::Bool(b)) => Ok(Some(b)),
        Some(BoolHelper::Str(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "" | "true" | "1" | "yes" | "t" => Ok(Some(true)),
            "false" | "0" | "no" | "f" => Ok(Some(false)),
            other => Err(serde::de::Error::custom(format!("invalid boolean value: '{other}'"))),
        },
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_various_date_formats() {
        let expected = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();

        assert_eq!(parse_date("2026-10-01").unwrap(), expected);
        assert_eq!(parse_date("2026-10-01T00:00:00").unwrap(), expected);
        assert_eq!(parse_date("2026-10-01T00:00:00Z").unwrap(), expected);
        assert_eq!(parse_date("2026-10-01T00:00:00+00:00").unwrap(), expected);
        assert_eq!(parse_date("2026-10-01T00:00:00.000Z").unwrap(), expected);
        assert_eq!(parse_date("2026-10-01 00:00:00").unwrap(), expected);
        assert_eq!(parse_date("2026/10/01").unwrap(), expected);
        assert_eq!(parse_date("  2026-10-01T00:00:00  ").unwrap(), expected);

        assert!(parse_date("").is_err());
        assert!(parse_date("invalid").is_err());
    }

    #[test]
    fn parses_various_datetime_formats() {
        let dt1 = parse_datetime("2026-10-01T00:00:00Z").unwrap();
        assert_eq!(dt1.to_rfc3339(), "2026-10-01T00:00:00+00:00");

        let dt2 = parse_datetime("2026-10-01T00:00:00").unwrap();
        assert_eq!(dt2.to_rfc3339(), "2026-10-01T00:00:00+00:00");

        let dt3 = parse_datetime("2026-10-01").unwrap();
        assert_eq!(dt3.to_rfc3339(), "2026-10-01T00:00:00+00:00");
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestQuery {
        #[serde(default, deserialize_with = "deserialize_opt_date")]
        date_from: Option<NaiveDate>,
        #[serde(default, deserialize_with = "deserialize_opt_date")]
        date_to: Option<NaiveDate>,
    }

    #[test]
    fn deserializes_query_string_variants() {
        let q: TestQuery =
            serde_urlencoded::from_str("date_from=2026-10-01T00%3A00%3A00&date_to=2026-10-14T00%3A00%3A00").unwrap();
        assert_eq!(q.date_from, Some(NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()));
        assert_eq!(q.date_to, Some(NaiveDate::from_ymd_opt(2026, 10, 14).unwrap()));

        let q_empty: TestQuery = serde_urlencoded::from_str("date_from=&date_to=").unwrap();
        assert_eq!(q_empty.date_from, None);
        assert_eq!(q_empty.date_to, None);

        let q_omitted: TestQuery = serde_urlencoded::from_str("").unwrap();
        assert_eq!(q_omitted.date_from, None);
        assert_eq!(q_omitted.date_to, None);
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestBoolQuery {
        #[serde(default, deserialize_with = "deserialize_opt_bool", alias = "is_due", alias = "due_only")]
        due_for_service: Option<bool>,
    }

    #[test]
    fn deserializes_opt_bool_variants() {
        let q1: TestBoolQuery = serde_urlencoded::from_str("due_for_service=true").unwrap();
        assert_eq!(q1.due_for_service, Some(true));

        let q2: TestBoolQuery = serde_urlencoded::from_str("due_for_service=1").unwrap();
        assert_eq!(q2.due_for_service, Some(true));

        let q3: TestBoolQuery = serde_urlencoded::from_str("due_for_service=").unwrap();
        assert_eq!(q3.due_for_service, Some(true));

        let q4: TestBoolQuery = serde_urlencoded::from_str("is_due=true").unwrap();
        assert_eq!(q4.due_for_service, Some(true));

        let q5: TestBoolQuery = serde_urlencoded::from_str("due_only=false").unwrap();
        assert_eq!(q5.due_for_service, Some(false));

        let q6: TestBoolQuery = serde_urlencoded::from_str("").unwrap();
        assert_eq!(q6.due_for_service, None);

        let q_json: TestBoolQuery = serde_json::from_str(r#"{"due_for_service": true}"#).unwrap();
        assert_eq!(q_json.due_for_service, Some(true));

        let q_json_null: TestBoolQuery = serde_json::from_str(r#"{"due_for_service": null}"#).unwrap();
        assert_eq!(q_json_null.due_for_service, None);
    }

    #[test]
    fn parses_various_time_formats() {
        assert_eq!(parse_time("09:30").unwrap(), NaiveTime::from_hms_opt(9, 30, 0).unwrap());
        assert_eq!(parse_time("09:30:45").unwrap(), NaiveTime::from_hms_opt(9, 30, 45).unwrap());
        assert_eq!(parse_time("09:30:45.500").unwrap(), NaiveTime::from_hms_milli_opt(9, 30, 45, 500).unwrap());
        // Date-time strings (ISO / datetime-local)
        assert_eq!(parse_time("2026-10-26T08:00").unwrap(), NaiveTime::from_hms_opt(8, 0, 0).unwrap());
        assert_eq!(parse_time("2026-10-26T08:00:00").unwrap(), NaiveTime::from_hms_opt(8, 0, 0).unwrap());
        assert_eq!(parse_time("2026-10-26T08:00:00Z").unwrap(), NaiveTime::from_hms_opt(8, 0, 0).unwrap());
        assert_eq!(parse_time("2026-10-26T13:30:00-04:00").unwrap(), NaiveTime::from_hms_opt(13, 30, 0).unwrap());
        // 12-hour AM/PM formats
        assert_eq!(parse_time("08:00 AM").unwrap(), NaiveTime::from_hms_opt(8, 0, 0).unwrap());
        assert_eq!(parse_time("01:15 PM").unwrap(), NaiveTime::from_hms_opt(13, 15, 0).unwrap());
        assert!(parse_time("").is_err());
        assert!(parse_time("invalid").is_err());
    }

    #[test]
    fn parses_end_of_day_correctly() {
        let eod = parse_datetime_end_of_day("2026-10-15").unwrap();
        assert_eq!(eod.to_rfc3339(), "2026-10-15T23:59:59.999999999+00:00");

        // Specific time should be preserved
        let specific = parse_datetime_end_of_day("2026-10-15T10:00:00Z").unwrap();
        assert_eq!(specific.to_rfc3339(), "2026-10-15T10:00:00+00:00");
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestPayload {
        #[serde(deserialize_with = "deserialize_time")]
        start_time: NaiveTime,
        #[serde(default, deserialize_with = "deserialize_opt_time")]
        end_time: Option<NaiveTime>,
        #[serde(default, deserialize_with = "deserialize_opt_datetime_utc")]
        effective_from: Option<DateTime<Utc>>,
        #[serde(default, deserialize_with = "deserialize_opt_datetime_end_of_day")]
        date_to: Option<DateTime<FixedOffset>>,
    }

    #[test]
    fn deserializes_json_payloads_with_loose_formats() {
        let json_str = r#"{
            "start_time": "09:00",
            "end_time": "17:00:00",
            "effective_from": "2026-10-15",
            "date_to": "2026-10-20"
        }"#;
        let p: TestPayload = serde_json::from_str(json_str).unwrap();
        assert_eq!(p.start_time, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        assert_eq!(p.end_time, Some(NaiveTime::from_hms_opt(17, 0, 0).unwrap()));
        assert_eq!(
            p.effective_from,
            Some(Utc.from_utc_datetime(&NaiveDate::from_ymd_opt(2026, 10, 15).unwrap().and_hms_opt(0, 0, 0).unwrap()))
        );
        assert_eq!(p.date_to.unwrap().to_rfc3339(), "2026-10-20T23:59:59.999999999+00:00");

        // Empty strings should deserialize to None
        let empty_json = r#"{
            "start_time": "08:00",
            "end_time": "",
            "effective_from": "",
            "date_to": ""
        }"#;
        let p_empty: TestPayload = serde_json::from_str(empty_json).unwrap();
        assert_eq!(p_empty.start_time, NaiveTime::from_hms_opt(8, 0, 0).unwrap());
        assert_eq!(p_empty.end_time, None);
        assert_eq!(p_empty.effective_from, None);
        assert_eq!(p_empty.date_to, None);
    }
}
