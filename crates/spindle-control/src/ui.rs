//! The observe UI, embedded at compile time from `ui/dist` (see `build.rs`).

include!(concat!(env!("OUT_DIR"), "/ui.rs"));

/// The page every other file of the bundle is loaded from.
pub(crate) const INDEX: &str = "index.html";

/// Whether this build carries the UI.
pub fn embedded() -> bool {
    file(INDEX).is_some()
}

/// The embedded file at `name`, relative to `ui/dist`.
pub(crate) fn file(name: &str) -> Option<&'static [u8]> {
    FILES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, bytes)| *bytes)
}

/// The index page with `?t=<token>` added to every `./<file>` it references that the bundle
/// holds, so the browser's requests for them carry the token as every request must.
pub(crate) fn index(token: &str) -> Option<String> {
    let text = std::str::from_utf8(file(INDEX)?).ok()?;
    let mut page = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("\"./") {
        let (before, from) = rest.split_at(start + 3);
        page.push_str(before);
        let end = from.find('"').unwrap_or(from.len());
        let (name, after) = from.split_at(end);
        page.push_str(name);
        if file(name).is_some() {
            page.push_str("?t=");
            page.push_str(token);
        }
        rest = after;
    }
    page.push_str(rest);
    Some(page)
}

/// The media type of a bundle file, by its extension.
pub(crate) fn content_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
