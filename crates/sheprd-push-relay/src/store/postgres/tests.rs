use super::*;

fn mode(url: &str) -> Result<SslMode> {
    Ok(encrypted(Config::from_str(url)?)?.get_ssl_mode())
}

#[test]
fn a_url_that_says_nothing_requires_tls() {
    assert_eq!(mode("postgres://u@db.example/r").unwrap(), SslMode::Require);
}

#[test]
fn prefer_cannot_fall_back_to_plain_text() {
    let url = "postgres://u@db.example/r?sslmode=prefer";
    assert_eq!(mode(url).unwrap(), SslMode::Require);
}

#[test]
fn plain_text_is_for_this_machine_only() {
    for url in [
        "postgres://u@127.0.0.1/r?sslmode=disable",
        "postgres://u@localhost/r?sslmode=disable",
        "postgres://u@[::1]/r?sslmode=disable",
    ] {
        assert_eq!(mode(url).unwrap(), SslMode::Disable, "{url}");
    }
    assert!(matches!(
        mode("postgres://u@db.example/r?sslmode=disable"),
        Err(Error::PostgresPlainText)
    ));
    assert!(matches!(
        mode("postgres://u@127.0.0.1,db.example/r?sslmode=disable"),
        Err(Error::PostgresPlainText)
    ));
}
