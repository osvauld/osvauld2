//! Two endpoints in one process over loopback: no relay, no lookup service, addresses handed
//! across directly — the same shape `OSVAULD_NET=local` gives two shells on one machine.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use iroh::EndpointAddr;
use serde::{Deserialize, Serialize};

use crate::endpoint::{Mode, bind, dial};
use crate::frames::{FIRST_FRAME, FrameBuf, send_frame};
use crate::{Error, Net};

const ALPN: &[u8] = b"osvauld/test/0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Msg {
    Hello { who: String, vv: Vec<u8> },
    Ack(u32),
}

fn secret(n: u8) -> [u8; 32] {
    let mut s = [0u8; 32];
    s[0] = n;
    s[31] = n;
    s
}

/// The address a local peer would get: the id and the loopback socket addresses.
fn local_addr(ep: &iroh::Endpoint) -> EndpointAddr {
    let addr = ep.addr();
    let ips = addr.ip_addrs().copied().collect::<Vec<SocketAddr>>();
    assert!(!ips.is_empty(), "a bound endpoint knows its direct addresses: {addr:?}");
    EndpointAddr::from_parts(addr.id, ips.into_iter().map(iroh::TransportAddr::Ip))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frames_round_trip_over_loopback() {
    let a = bind(&secret(1), Mode::Local, vec![ALPN.to_vec()], None).await.unwrap();
    let b = bind(&secret(2), Mode::Local, vec![ALPN.to_vec()], None).await.unwrap();
    let b_addr = local_addr(&b);
    let server = tokio::spawn(async move {
        let conn = b.accept().await.unwrap().accept().unwrap().await.unwrap();
        let (mut send, recv) = conn.accept_bi().await.unwrap();
        let mut frames = FrameBuf::new(recv);
        let hello: Msg = frames.recv(FIRST_FRAME).await.unwrap().unwrap();
        send_frame(&mut send, &Msg::Ack(7), FIRST_FRAME).await.unwrap();
        let end: Option<Msg> = frames.recv(FIRST_FRAME).await.unwrap();
        (hello, end, conn.remote_id())
    });
    let conn = dial(&a, b_addr, ALPN, Duration::from_secs(5)).await.unwrap();
    let (mut send, recv) = conn.open_bi().await.unwrap();
    let hello = Msg::Hello {
        who: "alice".into(),
        vv: vec![1, 2, 3],
    };
    send_frame(&mut send, &hello, FIRST_FRAME).await.unwrap();
    let mut frames = FrameBuf::new(recv);
    let ack: Msg = frames.recv(FIRST_FRAME).await.unwrap().unwrap();
    assert_eq!(ack, Msg::Ack(7));
    send.finish().unwrap();
    let (got, end, remote) = server.await.unwrap();
    assert_eq!(got, hello);
    assert_eq!(end, None, "a finished stream reads as a clean end");
    assert_eq!(remote, a.id(), "the server sees the dialer's device key");
}

/// A frame header over the cap ends the read at once: the body is never waited for or
/// buffered, so a stranger cannot make a host hold the cap's worth of bytes it never sends.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_header_over_the_cap_is_refused_before_the_body() {
    let a = bind(&secret(3), Mode::Local, vec![ALPN.to_vec()], None).await.unwrap();
    let b = bind(&secret(4), Mode::Local, vec![ALPN.to_vec()], None).await.unwrap();
    let b_addr = local_addr(&b);
    let server = tokio::spawn(async move {
        let conn = b.accept().await.unwrap().accept().unwrap().await.unwrap();
        let (_send, recv) = conn.accept_bi().await.unwrap();
        let mut frames = FrameBuf::new(recv);
        let r = tokio::time::timeout(Duration::from_secs(5), frames.recv::<Msg>(FIRST_FRAME)).await;
        (r, conn)
    });
    let conn = dial(&a, b_addr, ALPN, Duration::from_secs(5)).await.unwrap();
    let (mut send, _recv) = conn.open_bi().await.unwrap();
    // Only the header: a body one byte over the cap, never sent.
    send.write_all(&((FIRST_FRAME as u32 + 1).to_be_bytes())).await.unwrap();
    let (r, _conn) = server.await.unwrap();
    let err = r.expect("refused at once, not after a timeout").expect_err("over the cap");
    assert!(matches!(err, Error::Protocol(ref m) if m.contains("over the cap")), "{err}");
}

/// The body that follows an over-cap header is never read: the buffer holds the header alone
/// even when the peer sent the body in the same write.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_over_cap_body_sent_with_its_header_is_not_buffered() {
    let a = bind(&secret(8), Mode::Local, vec![ALPN.to_vec()], None).await.unwrap();
    let b = bind(&secret(9), Mode::Local, vec![ALPN.to_vec()], None).await.unwrap();
    let b_addr = local_addr(&b);
    let server = tokio::spawn(async move {
        let conn = b.accept().await.unwrap().accept().unwrap().await.unwrap();
        let (_send, recv) = conn.accept_bi().await.unwrap();
        let mut frames = FrameBuf::new(recv);
        let r = tokio::time::timeout(Duration::from_secs(5), frames.recv::<Msg>(FIRST_FRAME)).await;
        (r, frames.buffered(), conn)
    });
    let conn = dial(&a, b_addr, ALPN, Duration::from_secs(5)).await.unwrap();
    let (mut send, _recv) = conn.open_bi().await.unwrap();
    let mut bytes = (FIRST_FRAME as u32 * 4).to_be_bytes().to_vec();
    bytes.extend(std::iter::repeat_n(0xAB, 32 * 1024));
    send.write_all(&bytes).await.unwrap();
    let (r, buffered, _conn) = server.await.unwrap();
    assert!(r.unwrap().is_err(), "over the cap");
    assert_eq!(buffered, 4, "only the header was read");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dial_to_nowhere_times_out() {
    let a = bind(&secret(5), Mode::Local, vec![ALPN.to_vec()], None).await.unwrap();
    // TEST-NET-1 (RFC 5737): routable nowhere, so the dial hangs until our timeout.
    let nowhere = EndpointAddr::from_parts(
        iroh::SecretKey::from_bytes(&secret(6)).public(),
        [iroh::TransportAddr::Ip(SocketAddr::from((Ipv4Addr::new(192, 0, 2, 1), 1)))],
    );
    let r = dial(&a, nowhere, ALPN, Duration::from_millis(500)).await;
    assert!(matches!(r, Err(Error::Timeout)), "{r:?}");
}

#[test]
fn block_on_runs_from_a_plain_thread_and_addr_is_direct_in_local_mode() {
    let net = Net::bind(&secret(7), Mode::Local, vec![ALPN.to_vec()], None).unwrap();
    assert_eq!(net.id(), iroh::SecretKey::from_bytes(&secret(7)).public());
    let addr = net.addr(Duration::from_secs(2));
    assert!(addr.ip_addrs().next().is_some(), "{addr:?}");
    assert!(addr.relay_urls().next().is_none(), "local mode has no relay: {addr:?}");
    let answer = std::thread::spawn({
        let handle = net.handle().clone();
        move || handle.block_on(async { 41 + 1 })
    })
    .join()
    .unwrap();
    assert_eq!(answer, 42);
    assert_eq!(net.block_on(async { "ok" }).unwrap(), "ok");
    net.close();
}

#[test]
fn mode_reads_the_env() {
    // Set and restore: other tests in this binary read the same variable.
    let before = std::env::var("OSVAULD_NET").ok();
    for (value, mode) in [("local", Mode::Local), ("relay-only", Mode::RelayOnly), ("auto", Mode::Auto), ("", Mode::Auto), ("nonsense", Mode::Auto)] {
        unsafe { std::env::set_var("OSVAULD_NET", value) };
        assert_eq!(Mode::from_env(), mode, "{value:?}");
    }
    unsafe { std::env::remove_var("OSVAULD_NET") };
    assert_eq!(Mode::from_env(), Mode::Auto);
    if let Some(v) = before {
        unsafe { std::env::set_var("OSVAULD_NET", v) };
    }
}
