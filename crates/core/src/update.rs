//! Update check against the project's GitHub Releases. Read-only: one anonymous GET to the
//! public GitHub API, no personal data sent.

use anyhow::{bail, Result};
use serde_json::Value;
use std::time::Duration;

pub const REPO: &str = "Contraption8or/command-center";

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// e.g. "v0.3.0"
    pub tag: String,
    pub name: String,
    /// Release page on github.com.
    pub page_url: String,
    /// Direct download of the attached .exe, if the release has one.
    pub exe_url: Option<String>,
    /// Release notes (Markdown).
    pub notes: String,
}

/// "v1.2.3" / "1.2" → [1, 2, 3] / [1, 2, 0]. Anything after a '-' or '+' is ignored.
fn parse(v: &str) -> Option<[u64; 3]> {
    let core = v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut out = [0u64; 3];
    for (i, part) in core.split('.').enumerate() {
        if i >= 3 {
            break;
        }
        out[i] = part.parse().ok()?;
    }
    Some(out)
}

/// True when `tag` is a strictly newer version than `current`.
pub fn is_newer(current: &str, tag: &str) -> bool {
    matches!((parse(current), parse(tag)), (Some(c), Some(t)) if t > c)
}

pub fn release_from_json(v: &Value) -> Option<Release> {
    let tag = v.get("tag_name")?.as_str()?.to_string();
    let exe_url = v
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().find(|x| x["name"].as_str().is_some_and(|n| n.to_ascii_lowercase().ends_with(".exe"))))
        .and_then(|x| x["browser_download_url"].as_str())
        .map(str::to_string);
    Some(Release {
        name: v.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(&tag).to_string(),
        page_url: v.get("html_url").and_then(Value::as_str).unwrap_or_default().to_string(),
        notes: v.get("body").and_then(Value::as_str).unwrap_or_default().to_string(),
        exe_url,
        tag,
    })
}

/// Latest published release, or `None` if the repo has none yet. Blocking.
pub fn fetch_latest() -> Result<Option<Release>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let resp = agent
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("User-Agent", "command-center-update-check")
        .header("Accept", "application/vnd.github+json")
        .call()?;
    match resp.status().as_u16() {
        200 => {
            let v: Value = resp.into_body().read_json()?;
            Ok(release_from_json(&v))
        }
        404 => Ok(None),
        s => bail!("GitHub returned HTTP {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn version_compare() {
        assert!(is_newer("0.2.0", "v0.3.0"));
        assert!(is_newer("0.2.0", "v0.2.1"));
        assert!(is_newer("0.9.9", "1.0"));
        assert!(!is_newer("0.2.0", "v0.2.0"));
        assert!(!is_newer("0.3.0", "v0.2.9"));
        assert!(!is_newer("0.2.0", "nightly"));
        assert!(is_newer("0.2.0", "v0.3.0-beta.1"));
    }

    #[test]
    fn parses_release_json() {
        let v = json!({
            "tag_name": "v0.3.0", "name": "", "html_url": "https://github.com/x/y/releases/tag/v0.3.0",
            "body": "Notes", "assets": [
                {"name": "notes.txt", "browser_download_url": "https://a/notes.txt"},
                {"name": "command-center.exe", "browser_download_url": "https://a/command-center.exe"}
            ]
        });
        let r = release_from_json(&v).unwrap();
        assert_eq!((r.tag.as_str(), r.name.as_str()), ("v0.3.0", "v0.3.0"));
        assert_eq!(r.exe_url.as_deref(), Some("https://a/command-center.exe"));
    }
}
