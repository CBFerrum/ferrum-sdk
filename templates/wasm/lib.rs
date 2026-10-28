//! The {{name}} plugin. `plugin.json` declares what it receives (here: every {{entity}} just
//! before it is validated) and what it may do (nothing beyond that: no records, no hosts).

use ferrum_plugin_sdk::{Request, Value, json, plugin};

plugin!(handle);

fn handle(req: Request) -> Value {
    match (req.kind(), req.str("stage")) {
        ("hook", "before_validate") => tidy_title(&req),
        _ => json!({}),
    }
}

/// Collapses runs of white space in the title ("  Call   back " becomes "Call back").
fn tidy_title(req: &Request) -> Value {
    let title = req.get("doc")["title"].as_str().unwrap_or_default();
    let tidy = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if tidy == title { json!({}) } else { json!({"patch": {"title": tidy}}) }
}
