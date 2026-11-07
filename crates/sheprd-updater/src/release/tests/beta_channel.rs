use super::*;

fn release(tag: &str, prerelease: bool) -> serde_json::Value {
    serde_json::json!({"tag_name": tag, "draft": false, "prerelease": prerelease, "assets": [{
        "name": "update-manifest.json", "size": 100,
        "browser_download_url": format!(
            "https://github.com/penso/herdr-gpui/releases/download/{tag}/update-manifest.json"
        ),
    }]})
}

fn newest(releases: &[serde_json::Value]) -> Result<Release> {
    parse_releases(&serde_json::to_vec(releases).map_err(Error::ReleaseJson)?)
}

#[test]
fn stable_channel_never_accepts_a_prerelease() -> anyhow::Result<()> {
    let beta = serde_json::to_vec(&release("v20260920.2", true))?;
    assert!(matches!(parse_release(&beta), Err(Error::UnstableRelease)));
    Ok(())
}

#[test]
fn beta_channel_offers_the_highest_version_of_either_kind() -> anyhow::Result<()> {
    // A beta newer than the latest stable release wins.
    let found = newest(&[release("v20260921.1", true), release("v20260920.1", false)])?;
    assert_eq!(found.version, "20260921.1");
    assert!(found.prerelease);
    // A stable release newer than every beta wins too, so beta testers are
    // never held back on an abandoned beta.
    let found = newest(&[release("v20260920.1", true), release("v20260922.1", false)])?;
    assert_eq!(found.version, "20260922.1");
    // Listing order is creation order; version order decides, numerically.
    let found = newest(&[
        release("v20260920.9", true),
        release("v20260920.10", true),
        release("v20260919.11", false),
    ])?;
    assert_eq!(found.version, "20260920.10");
    Ok(())
}

#[test]
fn beta_channel_skips_drafts_and_foreign_tags_but_validates_the_choice() -> anyhow::Result<()> {
    let mut draft = release("v20260930.1", true);
    draft["draft"] = true.into();
    let found = newest(&[
        draft,
        release("v99999999.1-rc.1", true),
        release("nightly", true),
        release("v20260920.1", true),
    ])?;
    assert_eq!(found.version, "20260920.1");

    let mut untrusted = release("v20260921.1", true);
    untrusted["assets"][0]["browser_download_url"] = "https://evil.test/a".into();
    assert!(matches!(
        newest(&[untrusted, release("v20260920.1", false)]),
        Err(Error::ReleaseAsset)
    ));

    for releases in [vec![], vec![release("nightly", true)]] {
        assert!(matches!(newest(&releases), Err(Error::NoPublishedRelease)));
    }
    Ok(())
}

#[test]
fn beta_listing_is_bounded() {
    let oversized = vec![b' '; RELEASE_LIST_LIMIT + 1];
    assert!(matches!(
        parse_releases(&oversized),
        Err(Error::MetadataLimit)
    ));
}
