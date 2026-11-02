use super::*;

#[test]
fn the_helper_s_own_step_comes_back_typed_and_package_lines_stay_data() {
    assert_eq!(
        Progress::from_helper(Progress::Approve.to_string()),
        Progress::Approve
    );
    assert_eq!(
        Progress::from_helper("Unpacking herdr-gpui ...".into()),
        Progress::Package("Unpacking herdr-gpui ...".into())
    );
    assert_eq!(
        Progress::Homebrew("==> Downloading".into()).to_string(),
        "Homebrew: ==> Downloading"
    );
}
