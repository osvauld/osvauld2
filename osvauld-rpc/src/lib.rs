//! The bridge wire protocol: the `Request`/`Response` vocabulary spoken over a UDS socket —
//! 4-byte length prefix + JSON payload (`read_msg`/`write_msg` do the framing).
//!
//! Rewritten 2026-09-09 for shell2's automation story — the port of the old repo's control
//! server (`osvauld/scripts/osvauld/`, lessons only): a Python client logs in, builds
//! workspaces and app items, opens them, and drives/observes the running app. The
//! sthalam-era families (block docs, per-block `.lua` edits, imports/tables, PDF, pixel
//! screenshots) are gone — this shell hosts Lua apps.
//!
//! Nothing here executes anything: the shell's UI thread is the single authority
//! (docs/status.md item 1). The listener binds `$OSVAULD_SOCKET` (default `/tmp/osvauld.sock`)
//! and must create the socket `0600` — `Signup`/`Unlock` carry passphrases, so the socket is
//! as sensitive as they are.
//!
//! Items are addressed by id alone (ids are 128-bit random — unique in practice); the shell
//! resolves the owning workspace. `kind` is the lower-case item tag (`doc`, `app`, …).

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

/// One vault account on this device ([`Request::ListAccounts`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemSummary {
    pub id: String,
    pub ws_id: String,
    pub name: String,
    pub kind: String,
}

/// A passphrase on the wire. Serialises as a plain string (the protocol's shape) but never
/// prints: `Debug` is redacted, so request logs and test failures cannot leak credentials.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Passphrase(String);

impl std::fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Passphrase(<redacted>)")
    }
}

impl From<String> for Passphrase {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Passphrase {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::ops::Deref for Passphrase {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq<str> for Passphrase {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Request {
    /// Liveness probe — also how a test harness waits for the socket.
    Ping,

    // ── auth ────────────────────────────────────────────────────────────────────
    /// Accounts known to this device (the login screen's list).
    ListAccounts,
    /// Create an account. The response carries the recovery mnemonic — shown once, never again.
    Signup {
        name: String,
        passphrase: Passphrase,
    },
    /// Open the vault. `account` is an id or a name from [`Request::ListAccounts`].
    Unlock {
        account: String,
        passphrase: Passphrase,
    },
    /// Close the vault; the shell returns to its login screen.
    Lock,

    // ── workspaces ──────────────────────────────────────────────────────────────
    ListWorkspaces,
    CreateWorkspace {
        name: String,
    },

    // ── items ───────────────────────────────────────────────────────────────────
    ListItems {
        ws_id: String,
    },
    /// `kind` is the lower-case item tag: `doc`, `app`, …
    CreateItem {
        ws_id: String,
        name: String,
        kind: String,
    },
    /// Open (or focus) the item's tab — a running app is what the senses below address.
    OpenItem {
        item_id: String,
    },

    // ── files (an .app's source doc) ────────────────────────────────────────────
    /// The item's source-file paths (`main.lua`, …).
    ListFiles {
        item_id: String,
    },
    ReadFile {
        item_id: String,
        path: String,
    },
    /// Write one source file. If the item's tab is open, its VM reloads — the doc survives.
    WriteFile {
        item_id: String,
        path: String,
        content: String,
    },
    /// Force the item's staged reload: a whole second VM; doc cores and scratch survive.
    ReloadItem {
        item_id: String,
    },

    // ── app senses ──────────────────────────────────────────────────────────────
    /// The running app's `El` tree as JSON, no rects. The `id`s in it are `Click`'s targets.
    DumpTree {
        item_id: String,
    },
    /// Click the element with `el_id` — by id, not by coordinates.
    Click {
        item_id: String,
        el_id: String,
    },
    /// Set an input's value: fires the element's `on_input` map with `content`, exactly as
    /// typing-and-committing would.
    Type {
        item_id: String,
        el_id: String,
        content: String,
    },
    /// Press the input's `key`: `"enter"` or `"esc"`.
    Key {
        item_id: String,
        el_id: String,
        key: String,
    },
    /// The app's console (errors, newest last) — at most `last` lines.
    ReadConsole {
        item_id: String,
        last: usize,
    },
    /// Capture the next live frame. With no dimensions this is the exact visible window;
    /// width/height are logical points and scale controls physical pixels per point.
    Screenshot {
        item_id: String,
        width: Option<f32>,
        height: Option<f32>,
        scale: Option<f32>,
    },

    // ── app data (the running app's CRDT) ───────────────────────────────────────
    /// The runtime-data CRDT's named top-level containers as one deep JSON value.
    /// (A write family — SetText/RowAdd/RowSet/RowRemove — was specced here once and cut:
    /// writes that bypass the app's own handlers skip its checks and side effects. If a
    /// seeding/escape-hatch need ever becomes real, re-spec it against that need.)
    AppDataGet {
        item_id: String,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Response {
    #[serde(rename = "ok")]
    Ok { result: serde_json::Value },
    #[serde(rename = "err")]
    Err { message: String },
}

impl Response {
    pub fn ok(result: impl Serialize) -> Self {
        Response::Ok {
            result: serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Response::Err {
            message: message.into(),
        }
    }
}

// 4-byte big-endian length prefix + payload
pub fn write_msg<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "message too large"))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

pub fn read_msg<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests;
