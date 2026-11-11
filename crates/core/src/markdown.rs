//! Markdown → HTML for note previews.
//!
//! Raw HTML in a note is shown as text (never executed), and task-list checkboxes are
//! rendered with a `data-task` index so a click can toggle the matching `[ ]` in the source.

use pulldown_cmark::{html, CowStr, Event, Options, Parser};

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

pub fn to_html(src: &str) -> String {
    let mut task = 0usize;
    let events = Parser::new_ext(src, options()).map(|ev| match ev {
        Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
        Event::TaskListMarker(checked) => {
            let i = task;
            task += 1;
            Event::InlineHtml(CowStr::from(format!(
                r#"<input type="checkbox" data-task="{i}"{}> "#,
                if checked { " checked" } else { "" }
            )))
        }
        other => other,
    });
    let mut out = String::with_capacity(src.len() * 3 / 2);
    html::push_html(&mut out, events);
    out
}

/// Flip the `index`-th task-list checkbox in `src`. Returns `None` if there is no such item.
pub fn toggle_task(src: &str, index: usize) -> Option<String> {
    // Use the parser's own idea of which markers exist, so code blocks etc. are skipped.
    let mut seen = 0usize;
    let mut target = None;
    for (ev, range) in Parser::new_ext(src, options()).into_offset_iter() {
        if let Event::TaskListMarker(checked) = ev {
            if seen == index {
                target = Some((range, checked));
                break;
            }
            seen += 1;
        }
    }
    let (range, checked) = target?;
    let slice = &src[range.clone()];
    let at = slice.find(if checked { "[x]" } else { "[ ]" }).or_else(|| slice.find("[X]"))?;
    let start = range.start + at;
    let mut out = String::with_capacity(src.len());
    out.push_str(&src[..start]);
    out.push_str(if checked { "[ ]" } else { "[x]" });
    out.push_str(&src[start + 3..]);
    Some(out)
}

/// First meaningful line, stripped of Markdown markers: used as a note's title.
pub fn title(src: &str) -> String {
    let line = src.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let t = line.trim_start_matches(['#', '>', '-', '*', ' ']).trim();
    let t = t.strip_prefix("[ ]").or_else(|| t.strip_prefix("[x]")).unwrap_or(t).trim();
    plain(t)
}

/// Text after the title line, flattened to one line (for list snippets).
pub fn snippet(src: &str, max: usize) -> String {
    let mut lines = src.lines().map(str::trim).filter(|l| !l.is_empty());
    lines.next();
    let joined = lines
        .filter(|l| !l.chars().all(|c| matches!(c, '-' | '|' | ':' | ' ')))
        .map(|l| {
            if l.starts_with('|') {
                // Table row: keep the cell text only.
                plain(&l.split('|').map(str::trim).filter(|c| !c.is_empty()).collect::<Vec<_>>().join(" "))
            } else {
                plain(l.trim_start_matches(['#', '>', '-', '*', ' ']))
            }
        })
        .collect::<Vec<_>>()
        .join(" · ");
    if joined.chars().count() > max {
        joined.chars().take(max).collect::<String>() + "…"
    } else {
        joined
    }
}

fn plain(s: &str) -> String {
    s.replace("**", "").replace("__", "").replace('`', "").replace("[ ]", "☐").replace("[x]", "☑")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basics_and_escapes_html() {
        let h = to_html("# Hi\n\n**bold** <script>alert(1)</script>");
        assert!(h.contains("<h1>Hi</h1>"));
        assert!(h.contains("<strong>bold</strong>"));
        assert!(!h.contains("<script>"));
        assert!(h.contains("&lt;script&gt;"));
    }

    #[test]
    fn task_checkboxes_are_indexed() {
        let h = to_html("- [ ] a\n- [x] b");
        assert!(h.contains(r#"data-task="0">"#));
        assert!(h.contains(r#"data-task="1" checked>"#));
    }

    #[test]
    fn toggles_the_right_checkbox() {
        let src = "```\n- [ ] not a task\n```\n- [ ] one\n- [x] two\n";
        let once = toggle_task(src, 0).unwrap();
        assert!(once.contains("- [x] one"));
        assert!(once.contains("- [ ] not a task"));
        let twice = toggle_task(&once, 1).unwrap();
        assert!(twice.contains("- [ ] two"));
        assert_eq!(toggle_task(src, 5), None);
    }

    #[test]
    fn titles_and_snippets() {
        assert_eq!(title("\n\n# Ep 7 **edit** notes\nbody"), "Ep 7 edit notes");
        assert_eq!(title("- [ ] buy SSD"), "buy SSD");
        assert_eq!(title(""), "");
        assert_eq!(snippet("# T\n\n- one\n- two", 50), "one · two");
        assert_eq!(snippet("# T\n| a | b |\n|---|---|", 50), "a b");
    }
}
