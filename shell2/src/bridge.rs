//! The bridge thread: pure transport between a UDS client and the UI thread.
//!
//! Owns nothing but the socket and the framing — no vault, no UI state, never a mutation
//! (the sthalam pattern this replaces mutated the vault *on* the bridge thread). Each
//! connection carries exactly one request: read it, hand it to the UI thread as
//! [`Msg::Rpc`] over the event-loop proxy, wait for the reply, write it, close. Requests
//! execute inside `Shell::update` — the single authority — and the event delivery is what
//! wakes the on-demand loop, so answering also repaints.
//!
//! The socket is `$OSVAULD_SOCKET` (default `/tmp/osvauld.sock`). It is `0600` by
//! construction — bound under a private staging name, locked, then atomically renamed into
//! place — and a path that is live or not a socket is never clobbered: `Signup`/`Unlock`
//! carry passphrases, so the socket is as sensitive as they are.

use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use osvauld_rpc::{Request, Response, read_msg, write_msg};
use runtime::EventLoopProxy;

use crate::Msg;

/// How long a client waits for the UI thread before giving up with an error response —
/// long enough for a frame-bound handler, short enough that a stuck shell fails loudly.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the bridge waits for a connected client to send its request — a client that
/// connects and stalls must not wedge the (serial, one-request-per-connection) bridge.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

fn socket_path() -> PathBuf {
    std::env::var_os("OSVAULD_SOCKET")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/osvauld.sock"))
}

/// Bind and serve forever. Transport-only failures are printed, not fatal: the shell must
/// still run without a bridge. Binding itself — staged, locked to 0600, atomically renamed
/// into place — is `osvauld_rpc::bind_uds`, shared with kunki's bridge.
pub fn spawn(proxy: EventLoopProxy<Msg>) {
    let path = socket_path();
    match osvauld_rpc::bind_uds(&path) {
        Ok(listener) => {
            std::thread::spawn(move || serve(listener, proxy));
        }
        Err(e) => eprintln!("bridge: {e}; not serving"),
    }
}

fn serve(listener: UnixListener, proxy: EventLoopProxy<Msg>) {
    for conn in listener.incoming() {
        match conn {
            Ok(conn) => {
                if let Err(e) = handle(conn, &proxy) {
                    eprintln!("bridge: request failed: {e}");
                }
            }
            Err(e) => eprintln!("bridge: accept failed: {e}"),
        }
    }
}

/// One connection, one request, one reply.
fn handle(mut conn: UnixStream, proxy: &EventLoopProxy<Msg>) -> io::Result<()> {
    conn.set_read_timeout(Some(READ_TIMEOUT))?;
    let req: Request = serde_json::from_slice(&read_msg(&mut conn)?)
        .map_err(|e| io::Error::other(format!("bad request: {e}")))?;
    let (tx, rx) = mpsc::channel();
    eprintln!("DBG bridge: request read, sending event");
    proxy
        .send_event(Msg::Rpc(req, tx))
        .map_err(|e| io::Error::other(format!("shell event loop closed: {e}")))?;
    eprintln!("DBG bridge: event sent, waiting reply");
    let resp = rx
        .recv_timeout(REPLY_TIMEOUT)
        .unwrap_or_else(|_| Response::err("shell did not answer in 30s"));
    let payload =
        serde_json::to_vec(&resp).map_err(|e| io::Error::other(format!("response encode: {e}")))?;
    write_msg(&mut conn, &payload)
}
