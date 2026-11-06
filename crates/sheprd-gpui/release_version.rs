//! Parses a release's calendar version for the build script. No dependencies:
//! `tests/build_identity` recompiles the build script in an isolated offline crate.

/// Packs a release version `YYYYMMDD.COUNTER` as the Windows numeric version
/// `YYYY.MMDD.COUNTER.0`, or returns `None` when it has another shape or a part
/// does not fit in 16 bits.
pub fn numeric(version: &str) -> Option<u64> {
    let (date, counter) = version.split_once('.')?;
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if date.len() != 8 || !digits(date) || !digits(counter) {
        return None;
    }
    let (year, month_day) = date.split_at(4);
    let parts = [year, month_day, counter].map(str::parse::<u16>);
    let [Ok(year), Ok(month_day), Ok(counter)] = parts else {
        return None;
    };
    Some((u64::from(year) << 48) | (u64::from(month_day) << 32) | (u64::from(counter) << 16))
}
