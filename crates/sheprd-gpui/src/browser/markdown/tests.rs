use super::*;

fn body(source: &str) -> String {
    page(source, "notes.md")
}

#[test]
fn markdown_files_are_known_by_extension() {
    for name in ["a.md", "b.MD", "c.markdown", "d.Markdown"] {
        assert!(is_markdown(Path::new(name)), "{name}");
    }
    for name in ["a.txt", "md", "a.md.html", "README"] {
        assert!(!is_markdown(Path::new(name)), "{name}");
    }
}

#[test]
fn renders_github_flavored_markdown() {
    let html = body(
        "# Plan\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n- [ ] open\n\n\
         ~~old~~ text[^1]\n\n[^1]: a note\n",
    );
    assert!(html.contains("<h1 id=\"user-content-plan\">Plan</h1>"));
    assert!(html.contains("<table>"));
    assert!(html.contains("<td>1</td>"));
    assert!(html.contains("type=\"checkbox\""));
    assert!(html.contains("<del>old</del>"));
    assert!(html.contains("footnote-definition"));
    assert!(html.contains("href=\"#user-content-1\""));
    assert!(html.contains("id=\"user-content-1\""));
}

#[test]
fn safe_html_renders_and_scripts_are_removed() {
    let html = body(
        "<details><summary>More</summary>\n\nhidden <br> <span style=\"color: red\">red</span>\n\n\
         </details>\n\n<script>alert(1)</script>\n\n<style>body{}</style>\n\n\
         <img src=\"x.png\" onerror=\"alert(2)\"> <a href=\"javascript:alert(3)\">bad</a> \
         <iframe src=\"https://a.test/\"></iframe> <input type=\"text\">\n",
    );
    assert!(html.contains("<details>"));
    assert!(html.contains("<summary>More</summary>"));
    assert!(html.contains("<br>"));
    assert!(
        html.contains("<span style=\"color:red\">red</span>"),
        "{html}"
    );
    assert!(html.contains("src=\"x.png\""));
    assert!(html.contains("bad"));
    for removed in [
        "<script",
        "alert(",
        "onerror",
        "javascript:",
        "<iframe",
        "body{}",
        "type=\"text\"",
    ] {
        assert!(!html.contains(removed), "{removed}");
    }
}

#[test]
fn links_keep_only_safe_schemes() {
    let html = body(
        "[a](javascript:alert(1)) [b](JavaScript:x) [c](data:text/html,x) \
         [f](docs/next.md#top) [g](https://a.test/) [h](mailto:me@a.test) [i](#top)",
    );
    assert!(!html.to_ascii_lowercase().contains("javascript:"));
    assert!(!html.contains("data:text"));
    for kept in [
        "href=\"docs/next.md#user-content-top\"",
        "href=\"https://a.test/\"",
        "href=\"mailto:me@a.test\"",
        "href=\"#user-content-top\"",
    ] {
        assert!(html.contains(kept), "{kept}");
    }
    // The three refused links keep their text but lose their target.
    assert_eq!(html.matches("href=").count(), 4);
}

#[test]
fn images_load_from_the_folder_and_over_https() {
    let html = body("![local](img/a.png) ![web](https://a.test/b.png) ![bad](javascript:x)");
    assert!(html.contains("src=\"img/a.png\""));
    assert!(html.contains("src=\"https://a.test/b.png\""));
    assert!(!html.contains("javascript:"));
}

#[test]
fn ids_are_prefixed_so_notes_cannot_shadow_globals() {
    let html = body("<div id=\"__herdrAnnotate\">x</div>\n");
    assert!(html.contains("id=\"user-content-__herdrAnnotate\""));
    assert!(!html.contains("id=\"__herdrAnnotate\""));
}

#[test]
fn the_title_is_the_first_heading_or_the_file_name() {
    assert!(
        body("intro\n\n## a < b & `c`\n\n# later\n").contains("<title>a &lt; b &amp; c</title>")
    );
    assert!(body("# <b>x</b>\n").contains("<title>x</title>"));
    assert!(body("no heading here").contains("<title>notes.md</title>"));
    assert!(body("").contains("<title>notes.md</title>"));
    assert!(body("#   \n\ntext").contains("<title>notes.md</title>"));
    assert!(page("x", "a\"<b>.md").contains("<title>a&quot;&lt;b&gt;.md</title>"));
}

#[test]
fn the_page_is_self_contained_and_locked_down() {
    let html = body("hi");
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains(&format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP}\">"
    )));
    assert!(html.contains("<style>"));
    assert!(!html.contains("<script"));
    assert!(!html.contains("<link"));
}

#[test]
fn styles_keep_only_text_properties() {
    // Layout properties could stretch an invisible link over the whole page.
    let html = body(
        "<span style=\"color: red; position: fixed; inset: 0; z-index: 9; width: 100vw; \
         height: 100vh; display: block; background-image: url(https://a.test/t.png)\">x</span>\n",
    );
    let start = html.find("<span").unwrap_or_default();
    let span = &html[start
        ..html[start..]
            .find("</span>")
            .map_or(html.len(), |end| start + end)];
    assert!(span.contains("color"), "{span}");
    for removed in [
        "position",
        "inset",
        "z-index",
        "width",
        "height",
        "display",
        "background-image",
        "a.test",
    ] {
        assert!(!span.contains(removed), "{removed}: {span}");
    }
}

#[test]
fn headings_get_ids_that_section_links_reach() {
    let html = body(
        "[Jump](#next-steps) [Again](#details-1)\n\n## Next steps!\n\n## Details\n\n\
         ## Details\n\n## `run` it\n",
    );
    assert!(html.contains("href=\"#user-content-next-steps\""), "{html}");
    assert!(
        html.contains("<h2 id=\"user-content-next-steps\">"),
        "{html}"
    );
    assert!(html.contains("<h2 id=\"user-content-details\">"), "{html}");
    assert!(html.contains("href=\"#user-content-details-1\""), "{html}");
    assert!(
        html.contains("<h2 id=\"user-content-details-1\">"),
        "{html}"
    );
    assert!(html.contains("<h2 id=\"user-content-run-it\">"), "{html}");
}

#[test]
fn section_links_to_other_markdown_files_follow_the_prefix() {
    let html = body(
        "[a](next.md#top) [b](docs/Guide.MARKDOWN?x=1#intro) [c](page.html#top) \
         [d](https://a.test/x.md#top) [e](next.md) [g](#) [h](?view=notes#top)\n",
    );
    for href in [
        "href=\"next.md#user-content-top\"",
        "href=\"docs/Guide.MARKDOWN?x=1#user-content-intro\"",
        "href=\"page.html#top\"",
        "href=\"https://a.test/x.md#top\"",
        "href=\"next.md\"",
        "href=\"#\"",
        "href=\"?view=notes#user-content-top\"",
    ] {
        assert!(html.contains(href), "{href} in {html}");
    }
}

#[test]
fn repeated_headings_never_share_an_id() {
    let html = body("[Third](#details-1)\n\n## Details\n\n## Details\n\n## Details-1\n");
    assert!(html.contains("<h2 id=\"user-content-details\">"), "{html}");
    assert!(
        html.contains("<h2 id=\"user-content-details-1\">Details</h2>"),
        "{html}"
    );
    assert!(
        html.contains("<h2 id=\"user-content-details-1-1\">Details-1</h2>"),
        "{html}"
    );
}

#[test]
fn links_and_ids_that_already_carry_the_prefix_still_meet() {
    let html = body("[Jump](#user-content-top)\n\n<div id=\"user-content-top\"></div>\n");
    assert!(html.contains("href=\"#user-content-top\""), "{html}");
    assert!(html.contains("<div id=\"user-content-top\">"), "{html}");
}
