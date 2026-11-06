#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

mod catalog;
mod cjk;
mod locale;
mod negotiate;
mod transforms;

const FILES: &[File] = &[
    File {
        language: "en-US",
        feature: "greeting",
        source: "greeting-hello = Hello, { $name }!\ngreeting-tabs = { $count ->\n    [one] One tab\n   *[other] { $count } tabs\n}\n",
    },
    File {
        language: "en-US",
        feature: "menu",
        source: "menu-quit = Quit\n",
    },
    File {
        language: "fr",
        feature: "greeting",
        source: "greeting-hello = Bonjour, { $name } !\n",
    },
];

fn locale(tag: &str) -> Locale {
    tag.parse().unwrap()
}
