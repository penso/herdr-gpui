use super::*;

fn release(tag: &str, prerelease: bool, body: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"tag_name": tag, "draft": false, "prerelease": prerelease, "body": body, "assets": [{
        "name": "update-manifest.json", "size": 100,
        "browser_download_url": format!(
            "https://github.com/penso/herdr-gpui/releases/download/{tag}/update-manifest.json"
        ),
    }]})
}

/// GitHub sends `"body": null` for a release cut without notes. That is a
/// release with nothing to show, not malformed metadata.
#[test]
fn a_null_body_is_a_release_without_notes() -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(&release("v20260920.2", false, serde_json::Value::Null))?;
    let found = parse_release(&bytes)?;
    assert_eq!(found.version, "20260920.2");
    assert_eq!(found.body, None);
    Ok(())
}

/// The beta channel parses every listed release before choosing one, so a
/// single note-less release, even an old one, must not fail the check.
#[test]
fn a_null_body_anywhere_in_the_beta_listing_is_accepted() -> anyhow::Result<()> {
    let listing = [
        release("v20260921.1", true, "- Newest".into()),
        release("v20260901.1", false, serde_json::Value::Null),
    ];
    let found = parse_releases(&serde_json::to_vec(&listing)?)?;
    assert_eq!(found.body.as_deref(), Some("- Newest"));

    let listing = [
        release("v20260921.1", true, serde_json::Value::Null),
        release("v20260901.1", false, "- Older".into()),
    ];
    let found = parse_releases(&serde_json::to_vec(&listing)?)?;
    assert_eq!(found.version, "20260921.1");
    assert_eq!(found.body, None);
    Ok(())
}
