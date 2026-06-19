use std::net::SocketAddr;

use bytes::Bytes;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tracing::{debug, warn};

use crate::error::QueryError;
use crate::handler::build_response;

const MAX_UDP_SIZE: usize = 512;

pub(crate) struct Worker {
    id: usize,
    socket: UdpSocket,
}

impl Worker {
    pub(crate) fn bind(id: usize, addr: SocketAddr) -> Result<Self, std::io::Error> {
        let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        sock.set_reuse_port(true)?;
        sock.set_nonblocking(true)?;
        sock.bind(&addr.into())?;
        let socket = UdpSocket::from_std(sock.into())?;
        Ok(Self { id, socket })
    }

    pub(crate) async fn run(self) {
        let mut buf = [0u8; MAX_UDP_SIZE];
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, peer)) => {
                    let data = Bytes::copy_from_slice(&buf[..len]);
                    if let Err(e) = handle_query(&self.socket, data, peer).await {
                        warn!(worker = self.id, peer = %peer, err = %e, "query error");
                    }
                }
                Err(e) => warn!(worker = self.id, err = %e, "recv error"),
            }
        }
    }
}

async fn handle_query(socket: &UdpSocket, data: Bytes, peer: SocketAddr) -> Result<(), QueryError> {
    let query = dnser_proto::Message::parse(data)?;
    debug!(id = query.header.id, peer = %peer, "query");
    let response = build_response(query);
    let bytes = response.to_bytes()?;
    socket.send_to(&bytes, peer).await?;
    Ok(())
}
