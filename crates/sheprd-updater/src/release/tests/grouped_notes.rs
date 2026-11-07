use super::*;

fn release(tag: &str, prerelease: bool, body: &str) -> serde_json::Value {
    serde_json::json!({
        "tag_name": tag, "draft": false, "prerelease": prerelease,
        "body": body, "assets": []
    })
}

fn notes(
    releases: &[serde_json::Value],
    current: (u64, u64),
    target: &str,
    target_body: Option<&str>,
    channel: UpdateChannel,
) -> Result<Option<String>> {
    let listing = serde_json::to_vec(releases).map_err(Error::ReleaseJson)?;
    history_notes(&listing, current, target, target_body, channel)
}

#[test]
fn releases_between_the_installed_and_target_are_grouped_newest_first() -> anyhow::Result<()> {
    let listing = [
        release("v20260920.3", false, "- the target"),
        release("v20260920.1", false, "- skipped a version"),
        release("v20260919.2", false, "- older note"),
        release("v20260918.1", false, "- older still"),
        // Before the installed version, so not part of what this user gets.
        release("v20260901.1", false, "- ancient"),
    ];
    let grouped = notes(
        &listing,
        (20260917, 1),
        "20260920.3",
        Some("- the target"),
        UpdateChannel::Stable,
    )?
    .context("four releases carry notes, so they are grouped")?;
    let headings: Vec<&str> = grouped
        .lines()
        .filter(|line| line.starts_with("## "))
        .collect();
    assert_eq!(
        headings,
        [
            "## 20260920.3",
            "## 20260920.1",
            "## 20260919.2",
            "## 20260918.1"
        ],
        "{grouped}"
    );
    assert!(!grouped.contains("ancient"), "{grouped}");
    Ok(())
}

#[test]
fn a_single_release_is_not_grouped() -> anyhow::Result<()> {
    let listing = [release("v20260920.3", false, "- the target")];
    assert_eq!(
        notes(
            &listing,
            (20260920, 2),
            "20260920.3",
            Some("- the target"),
            UpdateChannel::Stable
        )?,
        None
    );
    Ok(())
}

#[test]
fn releases_without_notes_are_skipped_rather_than_shown_as_empty_headings() -> anyhow::Result<()> {
    let listing = [
        release("v20260920.2", false, "   "),
        release("v20260920.1", false, "- the only note"),
    ];
    let grouped = notes(
        &listing,
        (20260919, 1),
        "20260920.3",
        Some("- the target"),
        UpdateChannel::Stable,
    )?
    .context("two releases carry notes")?;
    assert!(!grouped.contains("20260920.2"), "{grouped}");
    assert!(grouped.contains("## 20260920.1"), "{grouped}");
    Ok(())
}

#[test]
fn a_stable_install_is_not_shown_prerelease_notes() -> anyhow::Result<()> {
    let listing = [
        release("v20260920.2", true, "- only in a beta"),
        release("v20260920.1", false, "- stable note"),
    ];
    let stable = notes(
        &listing,
        (20260919, 1),
        "20260920.3",
        Some("- the target"),
        UpdateChannel::Stable,
    )?
    .context("two stable releases carry notes")?;
    assert!(!stable.contains("only in a beta"), "{stable}");
    assert!(stable.contains("stable note"), "{stable}");

    // The same release is part of what a beta installation was offered.
    let beta = notes(
        &listing,
        (20260919, 1),
        "20260920.3",
        Some("- the target"),
        UpdateChannel::Beta,
    )?
    .context("two releases carry notes")?;
    assert!(beta.contains("only in a beta"), "{beta}");
    Ok(())
}

#[test]
fn drafts_and_foreign_tags_never_reach_the_grouped_notes() -> anyhow::Result<()> {
    let mut draft = release("v20260920.2", false, "- unpublished");
    draft["draft"] = true.into();
    let listing = [
        draft,
        release("nightly", false, "- not a version"),
        release("v20260920.1-rc.1", false, "- not a calendar version"),
        release("v20260920.1", false, "- published"),
    ];
    let grouped = notes(
        &listing,
        (20260919, 1),
        "20260920.3",
        Some("- the target"),
        UpdateChannel::Stable,
    )?
    .context("two published calendar releases carry notes")?;
    assert!(!grouped.contains("unpublished"), "{grouped}");
    assert!(!grouped.contains("not a version"), "{grouped}");
    assert!(!grouped.contains("rc.1"), "{grouped}");
    Ok(())
}

#[test]
fn the_grouped_list_is_capped() -> anyhow::Result<()> {
    let listing: Vec<serde_json::Value> = (1..=40)
        .map(|counter| release(&format!("v20260920.{counter}"), false, "- note"))
        .collect();
    let grouped = notes(
        &listing,
        (20260901, 1),
        "20260921.1",
        Some("- the target"),
        UpdateChannel::Stable,
    )?
    .context("many releases carry notes")?;
    let headings = grouped
        .lines()
        .filter(|line| line.starts_with("## "))
        .count();
    assert_eq!(headings, MAX_GROUPED_RELEASES, "{grouped}");
    // The cap keeps the newest, since those are the ones being installed.
    assert!(grouped.starts_with("## 20260921.1"), "{grouped}");
    Ok(())
}

fn release_with_assets(tag: &str, body: &str, assets: usize) -> serde_json::Value {
    let assets: Vec<serde_json::Value> = (0..assets)
        .map(|index| {
            let name = format!("Herdr-{tag}-target{index}-setup.exe");
            serde_json::json!({
                "name": name,
                "size": 12_345_678u64,
                "browser_download_url": format!(
                    "https://github.com/penso/herdr-gpui/releases/download/{tag}/{name}"
                ),
            })
        })
        .collect();
    serde_json::json!({
        "tag_name": tag, "draft": false, "prerelease": false, "body": body, "assets": assets
    })
}

/// A published release carries more than a hundred assets. A full history page
/// has to fit its own budget at that size, or the request fails and the grouped
/// notes fall back to the target's alone without saying so.
#[test]
fn a_full_page_of_published_releases_fits_the_history_budget() -> anyhow::Result<()> {
    let listing: Vec<serde_json::Value> = (1..=MAX_GROUPED_RELEASES)
        .map(|index| release_with_assets(&format!("v20260920.{index}"), "- note", 116))
        .collect();
    let bytes = serde_json::to_vec(&listing).map_err(Error::ReleaseJson)?;
    assert!(bytes.len() < HISTORY_LIMIT, "{} bytes", bytes.len());
    let grouped = notes(
        &listing,
        (20260919, 1),
        "20260921.1",
        Some("- the target"),
        UpdateChannel::Stable,
    )?
    .context("a full page of releases carries notes")?;
    assert!(grouped.starts_with("## 20260921.1"), "{}", &grouped[..40]);
    Ok(())
}

/// The cap, not the notes, ends the list, so the text says so and links to the
/// rest. A silent stop would read as the whole history.
#[test]
fn a_capped_history_says_what_it_left_out() -> anyhow::Result<()> {
    let listing: Vec<serde_json::Value> = (1..=30)
        .map(|index| release(&format!("v20260920.{index}"), false, "- note"))
        .collect();
    let grouped = notes(
        &listing,
        (20260901, 1),
        "20260921.1",
        Some("- the target"),
        UpdateChannel::Stable,
    )?
    .context("more releases than the cap")?;
    assert!(
        grouped.contains("Showing the newest 20 releases"),
        "{grouped}"
    );
    assert!(grouped.contains(RELEASES_PAGE), "{grouped}");
    Ok(())
}

/// Long bodies stop short of the renderer's bounds, and the cutoff is announced.
/// Left to the renderer, the same body would simply end part-way with no sign.
#[test]
fn long_release_bodies_stop_before_the_renderer_would_cut() -> anyhow::Result<()> {
    let body = (1..=40)
        .map(|line| format!("- change {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let listing: Vec<serde_json::Value> = (1..=20)
        .map(|index| release(&format!("v20260920.{index}"), false, &body))
        .collect();
    let grouped = notes(
        &listing,
        (20260901, 1),
        "20260921.1",
        Some(&body),
        UpdateChannel::Stable,
    )?
    .context("many long releases")?;
    assert!(
        grouped.lines().count() <= MAX_GROUPED_LINES,
        "{} lines",
        grouped.lines().count()
    );
    assert!(grouped.contains("Showing the newest"), "{grouped}");
    let headings = grouped
        .lines()
        .filter(|line| line.starts_with("## "))
        .count();
    assert!(headings < 20, "{headings} sections fit forty-line bodies");
    Ok(())
}
