//! The Anthropic API key lives in Windows Credential Manager (never in the database or in
//! backups). `ANTHROPIC_API_KEY` in the environment is used when no key is stored.

const SERVICE: &str = "CommandCenter";
const ACCOUNT: &str = "anthropic-api-key";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Source {
    CredentialManager,
    Environment,
}

fn entry() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT)
}

pub fn load() -> Option<(String, Source)> {
    if let Some(k) = entry().ok().and_then(|e| e.get_password().ok()).filter(|k| !k.trim().is_empty()) {
        return Some((k, Source::CredentialManager));
    }
    std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.trim().is_empty()).map(|k| (k, Source::Environment))
}

pub fn save(key: &str) -> Result<(), String> {
    let key = key.trim();
    if !key.starts_with("sk-ant-") {
        return Err("That doesn't look like an Anthropic API key (they start with sk-ant-).".into());
    }
    entry().and_then(|e| e.set_password(key)).map_err(|e| format!("Couldn't save to Credential Manager: {e}"))
}

pub fn remove() -> Result<(), String> {
    match entry().and_then(|e| e.delete_credential()) {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Couldn't remove the key: {e}")),
    }
}

/// "sk-ant-…a1b2" for display.
pub fn masked(key: &str) -> String {
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("sk-ant-…{tail}")
}
