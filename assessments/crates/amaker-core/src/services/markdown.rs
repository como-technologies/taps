//! Markdown → HTML helper. Shared so the binaries don't drift on their
//! rendering options (GFM tables, footnotes, etc. via `Options::all()`).

use pulldown_cmark::{Options, Parser, html};

/// Render a Markdown string to an HTML fragment using GFM-style extensions.
pub fn markdown_to_html(md: &str) -> String {
    let parser = Parser::new_ext(md, Options::all());
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}
