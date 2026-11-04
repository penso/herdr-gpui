use crate::http::moved_to;

const MOVED: &str = "https://api.github.com/repositories/1/dependabot/alerts?state=open";

#[test]
fn a_moved_repository_continues_at_the_api_address_github_names() {
    for status in [301, 302, 307, 308] {
        assert_eq!(moved_to(status, Some(MOVED)), Some(MOVED));
    }
}

#[test]
fn only_a_redirect_with_a_location_moves_a_read() {
    assert_eq!(moved_to(200, Some(MOVED)), None);
    assert_eq!(moved_to(404, Some(MOVED)), None);
    assert_eq!(moved_to(301, None), None);
}

#[test]
fn a_redirect_never_carries_the_token_off_the_api() {
    for url in [
        "https://example.com/repositories/1",
        "https://api.github.com.example.com/repositories/1",
        "https://api.github.com@example.com/repositories/1",
        "https://api.github.com:8443/repositories/1",
        "http://api.github.com/repositories/1",
        "https://uploads.github.com/repositories/1",
        "/repositories/1",
        "https://api.github.com",
    ] {
        assert_eq!(moved_to(301, Some(url)), None, "{url}");
    }
}
