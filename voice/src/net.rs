use std::time::Duration;

use iroh::Endpoint;
use iroh::endpoint::{Connection, presets};
use iroh_tickets::endpoint::EndpointTicket;

use crate::Error;

pub const ALPN: &[u8] = b"osvauld/voice/0";

pub async fn listen() -> Result<(Endpoint, Connection), Error> {
    let ep = Endpoint::builder(presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    // Waiting puts the relay URL in the ticket, so it also dials across NATs; offline, the
    // timeout lets a local-only ticket through.
    let _ = tokio::time::timeout(Duration::from_secs(3), ep.online()).await;
    eprintln!(
        "on the other peer run:\n  voice dial {}",
        EndpointTicket::new(ep.addr())
    );
    let incoming = ep.accept().await.ok_or(Error::Closed)?;
    let conn = incoming.accept()?.await?;
    eprintln!("call from {}", conn.remote_id());
    Ok((ep, conn))
}

pub async fn dial(ticket: EndpointTicket) -> Result<(Endpoint, Connection), Error> {
    let ep = Endpoint::builder(presets::N0).bind().await?;
    let addr = ticket.endpoint_addr().clone();
    let id = addr.id;
    let conn = ep.connect(addr, ALPN).await?;
    eprintln!("connected to {id}");
    Ok((ep, conn))
}
