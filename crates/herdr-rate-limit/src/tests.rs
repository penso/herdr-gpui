use super::*;

#[test]
fn retry_minute_rounds_any_part_of_a_minute_up() -> anyhow::Result<()> {
    let at = |text: &str| -> anyhow::Result<DateTime<Utc>> {
        Ok(DateTime::parse_from_rfc3339(text)?.with_timezone(&Utc))
    };
    for (reset, expected) in [
        ("2026-10-13T12:34:00Z", "2026-10-13T12:34:00Z"),
        ("2026-10-13T12:34:00.500Z", "2026-10-13T12:35:00Z"),
        ("2026-10-13T12:34:00.000000001Z", "2026-10-13T12:35:00Z"),
        ("2026-10-13T12:34:01Z", "2026-10-13T12:35:00Z"),
        ("2026-10-13T12:34:59.999Z", "2026-10-13T12:35:00Z"),
        ("2026-10-13T23:59:30Z", "2026-10-14T00:00:00Z"),
    ] {
        assert_eq!(retry_minute(at(reset)?), at(expected)?, "{reset}");
    }
    Ok(())
}
