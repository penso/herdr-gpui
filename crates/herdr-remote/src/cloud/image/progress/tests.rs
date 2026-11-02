use super::*;

fn after(lines: &[&str]) -> Progress {
    let mut progress = Progress::default();
    for line in lines {
        progress.line(line);
    }
    progress
}

#[test]
fn buildkit_plain_progress_numbers_its_steps() {
    let progress = after(&[
        "#1 [internal] load build definition from Dockerfile",
        "#5 [build 2/4] RUN apt-get update",
        "#5 0.512 Get:1 http://deb.debian.org bookworm InRelease",
    ]);
    assert_eq!(progress.step, Some((2, 4)));
    assert_eq!(
        progress.status,
        "#5 0.512 Get:1 http://deb.debian.org bookworm InRelease"
    );
    assert_eq!(progress.fraction(), Some(0.25));
}

#[test]
fn podman_and_the_classic_builder_number_theirs_too() {
    let podman = after(&["[1/2] STEP 3/5: RUN make"]);
    assert_eq!(podman.step, Some((3, 5)));
    assert_eq!(podman.status, "RUN make");
    let classic = after(&["Step 4/6 : COPY . ."]);
    assert_eq!(classic.step, Some((4, 6)));
    assert_eq!(classic.status, "COPY . .");
}

#[test]
fn a_build_without_numbers_shows_activity_only() {
    let progress = after(&["building", ""]);
    assert_eq!(progress.fraction(), None);
    assert_eq!(progress.status, "building");
    let long = after(&[&"x".repeat(500)]);
    assert_eq!(long.status.chars().count(), 120);
    assert_eq!(
        Progress {
            step: Some((1, 0)),
            status: String::new()
        }
        .fraction(),
        None
    );
}
