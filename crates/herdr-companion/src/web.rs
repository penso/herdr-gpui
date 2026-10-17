//! The installable web app, compiled into the binary so the companion stays a
//! single file. Assets are public; every API call they make carries the token
//! the person types into the app once.

/// The asset served for a request path, with its media type.
pub(crate) fn asset(path: &str) -> Option<(&'static str, &'static [u8])> {
    Some(match path {
        "/" | "/index.html" => (
            "text/html; charset=utf-8",
            include_bytes!("../web/index.html"),
        ),
        "/app.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/app.js"),
        ),
        "/app.css" => ("text/css; charset=utf-8", include_bytes!("../web/app.css")),
        // Served from the root so its scope covers the whole app.
        "/sw.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/sw.js"),
        ),
        "/manifest.webmanifest" => (
            "application/manifest+json",
            include_bytes!("../web/manifest.webmanifest"),
        ),
        "/icon.svg" => ("image/svg+xml", include_bytes!("../web/icon.svg")),
        "/icon-192.png" => ("image/png", include_bytes!("../web/icon-192.png")),
        "/icon-512.png" => ("image/png", include_bytes!("../web/icon-512.png")),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_the_app_shell_and_nothing_else() {
        for path in [
            "/",
            "/app.js",
            "/app.css",
            "/sw.js",
            "/manifest.webmanifest",
            "/icon.svg",
            "/icon-192.png",
            "/icon-512.png",
        ] {
            let (_, body) = asset(path).unwrap_or_else(|| panic!("{path} missing"));
            assert!(!body.is_empty(), "{path}");
        }
        assert!(asset("/v1/requests").is_none());
        assert!(asset("/../Cargo.toml").is_none());
    }

    #[test]
    fn the_shell_loads_only_same_origin_files() {
        let html = std::str::from_utf8(asset("/").map(|(_, body)| body).unwrap_or_default())
            .unwrap_or_default();
        assert!(html.contains("src=\"/app.js\""));
        assert!(
            !html.contains("<script>"),
            "inline scripts would violate the CSP"
        );
        assert!(!html.contains("http://") && !html.contains("https://"));
    }
}
