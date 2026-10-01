use std::net::SocketAddr;

use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, TransportAddr};

use crate::Error;

pub const ALPN: &[u8] = b"osvauld/voice/0";

pub async fn listen() -> Result<(Endpoint, Connection), Error> {
    let ep = Endpoint::builder(presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    let addrs: Vec<String> = ep.addr().ip_addrs().map(|a| a.to_string()).collect();
    eprintln!(
        "on the other peer run:\n  voice dial {} {}",
        ep.id(),
        addrs.join(" ")
    );
    let incoming = ep.accept().await.ok_or(Error::Closed)?;
    let conn = incoming.accept()?.await?;
    eprintln!("call from {}", conn.remote_id());
    Ok((ep, conn))
}

pub async fn dial(id: EndpointId, addrs: Vec<SocketAddr>) -> Result<(Endpoint, Connection), Error> {
    let ep = Endpoint::builder(presets::N0).bind().await?;
    let addr = EndpointAddr::from_parts(id, addrs.into_iter().map(TransportAddr::Ip));
    let conn = ep.connect(addr, ALPN).await?;
    eprintln!("connected to {id}");
    Ok((ep, conn))
}
