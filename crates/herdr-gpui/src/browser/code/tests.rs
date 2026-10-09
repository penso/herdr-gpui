#![allow(clippy::unwrap_used)]
use super::*;
use core::prelude::v1::test;

fn url(text: &str) -> WebUrl {
    WebUrl::try_from(text).unwrap()
}

#[test]
fn a_page_loads_where_it_was_with_the_configured_token() {
    let configured = url("http://127.0.0.1:8000/?tkn=new");
    // The server redirected the token away; the folder stays.
    assert_eq!(
        with_token(&url("http://127.0.0.1:8000/?folder=/x"), &configured),
        url("http://127.0.0.1:8000/?folder=/x&tkn=new")
    );
    // An old token is replaced, not repeated.
    assert_eq!(
        with_token(
            &url("http://127.0.0.1:8000/?tkn=old&folder=/x"),
            &configured
        ),
        url("http://127.0.0.1:8000/?folder=/x&tkn=new")
    );
    assert_eq!(
        with_token(&url("http://127.0.0.1:8000/"), &configured),
        url("http://127.0.0.1:8000/?tkn=new")
    );
}

#[test]
fn a_page_without_a_configured_token_or_on_another_server_is_left_alone() {
    let page = url("http://127.0.0.1:8000/?folder=/x");
    assert_eq!(with_token(&page, &url("http://127.0.0.1:8000/")), page);
    assert_eq!(
        with_token(&page, &url("http://127.0.0.1:9000/?tkn=new")),
        page
    );
}

#[test]
fn a_new_page_opens_the_folder_keeping_the_token() {
    assert_eq!(
        with_folder(&url("http://127.0.0.1:8000/?tkn=x"), "/Users/me/my project"),
        url("http://127.0.0.1:8000/?tkn=x&folder=%2FUsers%2Fme%2Fmy+project")
    );
    // A folder already named is replaced, not repeated.
    assert_eq!(
        with_folder(&url("http://127.0.0.1:8000/?folder=/old&tkn=x"), "/new"),
        url("http://127.0.0.1:8000/?tkn=x&folder=%2Fnew")
    );
    assert_eq!(
        with_folder(&url("http://127.0.0.1:8000/"), "/new"),
        url("http://127.0.0.1:8000/?folder=%2Fnew")
    );
}
