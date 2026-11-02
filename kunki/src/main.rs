//! Kunki's boot sequence, two ways. Bare `kunki`: open the node's account, print a
//! connection ticket, then serve the socket bridge — the pre-pivot node the chat suite's
//! node mode still runs. `kunki --data DIR host --join LINK --accept WS`: the headless
//! appointed host of laptop-host step 7, all of it in `kunki::headless`.
//!
//! The node keeps its identity in a `vault` account, the same store shell2 uses, and its
//! grants beside it. The ticket's text form belongs to `courier`, not here — a format only
//! the node could write would be one nothing else could read. The ticket is printed on
//! every boot (harmless once an admin exists — the bootstrap claim it carries is then
//! refused), then the process blocks in `bridge::serve_forever`: publishing and sync land as
//! more `Request` variants there, not as more of this boot sequence.

use std::path::PathBuf;
use std::process::ExitCode;

use kunki::headless::{self, Args, Command, USAGE};
use kunki::push::LiveRegistry;
use kunki::{NodeError, bridge};
use zeroize::Zeroizing;

// Not `main() -> Result`: that prints the Debug form, so every message these errors carry
// for the person running the node was replaced by its variant name.
fn main() -> ExitCode {
    let env_data = std::env::var_os("OSVAULD_KUNKI_DIR").map(PathBuf::from);
    let args = match headless::parse_args(std::env::args().skip(1), env_data) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("kunki: {e}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("kunki: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), NodeError> {
    let passphrase = Zeroizing::new(
        std::env::var("OSVAULD_KUNKI_PASSPHRASE").map_err(|_| NodeError::NoPassphrase)?,
    );
    match args.command {
        Command::Host { link, accept } => headless::run(args.data, &passphrase, &link, &accept),
        Command::Node => node(args.data, &passphrase),
    }
}

fn node(data: PathBuf, passphrase: &str) -> Result<(), NodeError> {
    let vault = headless::open(data, passphrase)?;
    // Signed inside the vault: the node's key never reaches this binary.
    let ticket = vault
        .with_signer(|signer| {
            courier::issue_connection_ticket(signer, headless::now_secs(), "kunki")
        })
        .ok_or(NodeError::Locked)??;
    println!("{}", ticket.to_text()?);
    bridge::serve_forever(vault, LiveRegistry::new())?;
    Ok(())
}
