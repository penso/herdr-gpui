//! A saved token never makes an instance its remotes read over plain HTTP
//! look like an HTTPS one, so its card keeps saying to change the remote.
use super::*;

#[test]
fn a_saved_token_keeps_the_remotes_plain_http() {
    let hosts = merge_hosts(
        [read("code.internal:8080", Scheme::Http, Some("/src/app"))],
        ["code.internal:8080".to_owned(), "gitlab.com".to_owned()],
        &[],
    );
    let listed = &hosts["code.internal:8080"];
    assert_eq!(listed.api.scheme, Scheme::Http);
    assert_eq!(listed.checkout, Some(PathBuf::from("/src/app")));
    // A saved instance no remote reads is still listed, over HTTPS.
    assert_eq!(hosts["gitlab.com"].api.scheme, Scheme::Https);
}
