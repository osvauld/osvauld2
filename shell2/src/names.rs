//! Xnet names: a bare name → the public invite ticket registered under it on Aptos
//! (`xnet_names/`, `docs/design/xnet-names.md`). Read-only — a view call, free, no key.

use std::time::Duration;

/// Devnet is wiped periodically and the redeployed registry gets a new address, so both are
/// overridable rather than baked in.
const DEVNET_URL: &str = "https://api.devnet.aptoslabs.com/v1";
const DEVNET_REGISTRY: &str = "0xd1d80a373ff509dcccd3edf720a3d3548edfb37fb9d9bd693b69cbcf71abcfa3";

/// The contract's own rule (`names::valid_name`), checked here so a typo never costs a round
/// trip and the bar can tell a name from a mangled ticket.
pub fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 63
        && !text.starts_with('-')
        && !text.ends_with('-')
        && text
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

pub fn resolve(name: &str) -> Result<String, String> {
    let url = std::env::var("OSVAULD_APTOS_URL").unwrap_or_else(|_| DEVNET_URL.to_string());
    let registry =
        std::env::var("OSVAULD_NAMES_REGISTRY").unwrap_or_else(|_| DEVNET_REGISTRY.to_string());
    let body = serde_json::json!({
        "function": format!("{registry}::names::resolve"),
        "type_arguments": [],
        "arguments": [name],
    });
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut resp = agent
        .post(format!("{url}/view"))
        // Not `send_json`: Aptos answers 415 to its `application/json; charset=utf-8`.
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|e| format!("resolving '{name}': {e}"))?;
    if !resp.status().is_success() {
        let detail = resp.body_mut().read_to_string().unwrap_or_default();
        // The API names the contract's abort constant in its message.
        if detail.contains("E_NOT_FOUND") {
            return Err(format!("no name '{name}'"));
        }
        return Err(format!("resolving '{name}': {} {detail}", resp.status()));
    }
    // `[owner, ticket]`.
    let (_owner, ticket): (String, String) = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("resolving '{name}': {e}"))?;
    Ok(ticket)
}

#[cfg(test)]
mod tests;
