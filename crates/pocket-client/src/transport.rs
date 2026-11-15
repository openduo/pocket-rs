// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
use pocket_core::{Error, Result, api::Request, pairing::Endpoint};
use std::{
    io::Read,
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};
use tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig, stream::MaybeTlsStream};

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}
pub struct Network {
    client: reqwest::blocking::Client,
}
impl Network {
    pub fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| Error::new("transport_error", "HTTP client initialization failed"))?;
        Ok(Self { client })
    }
    pub fn request(
        &self,
        endpoint: &Endpoint,
        request: &Request,
        timeout_ms: u64,
    ) -> Result<Response> {
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .map_err(|_| Error::invalid("Invalid HTTP method"))?;
        let mut call = self
            .client
            .request(method, endpoint.url(&request.path)?)
            .timeout(Duration::from_millis(timeout_ms.clamp(1, 60_000)));
        for (key, value) in &request.headers {
            call = call.header(key, value);
        }
        let response = call
            .body(request.body.clone())
            .send()
            .map_err(|_| Error::new("transport_error", "HTTP request failed"))?;
        let status = response.status().as_u16();
        let mut body = Vec::new();
        response
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut body)
            .map_err(|_| Error::new("transport_error", "HTTP body interrupted"))?;
        if body.len() > 16 * 1024 * 1024 {
            return Err(Error::new("limit_exceeded", "HTTP body exceeds limit"));
        }
        Ok(Response { status, body })
    }
}

pub struct Socket {
    inner: tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
}
impl Socket {
    pub fn connect(endpoint: &Endpoint, request: &Request) -> Result<Self> {
        let mut url = url::Url::parse(&endpoint.url(&request.path)?)
            .map_err(|_| Error::invalid("Invalid live URL"))?;
        url.set_scheme(if endpoint.tls { "wss" } else { "ws" })
            .map_err(|_| Error::invalid("Invalid live scheme"))?;
        let mut upgrade = url
            .as_str()
            .into_client_request()
            .map_err(|_| Error::invalid("Invalid WebSocket request"))?;
        for (key, value) in &request.headers {
            upgrade.headers_mut().insert(
                key.parse::<tungstenite::http::HeaderName>()
                    .map_err(|_| Error::invalid("Invalid header name"))?,
                value
                    .parse()
                    .map_err(|_| Error::invalid("Invalid header value"))?,
            );
        }
        let addresses = (endpoint.host.as_str(), endpoint.port)
            .to_socket_addrs()
            .map_err(|_| Error::new("transport_error", "Live host lookup failed"))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut connected = None;
        for address in addresses.take(8) {
            let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
                break;
            };
            if let Ok(stream) = TcpStream::connect_timeout(&address, remaining) {
                connected = Some(stream);
                break;
            }
        }
        let stream =
            connected.ok_or_else(|| Error::new("transport_error", "Live connection failed"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .map_err(io_error)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(8)))
            .map_err(io_error)?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(1024 * 1024))
            .max_frame_size(Some(1024 * 1024));
        let (inner, _) = tungstenite::client_tls_with_config(upgrade, stream, Some(config), None)
            .map_err(|e| match e {
            tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))
                if response.status().as_u16() == 401 =>
            {
                Error::new("unauthorized", "Live upgrade rejected device credential")
            }
            _ => Error::new("transport_error", "WebSocket handshake failed"),
        })?;
        Ok(Self { inner })
    }
    pub fn read(&mut self) -> Result<tungstenite::Message> {
        self.inner.read().map_err(|e| match e {
            tungstenite::Error::Io(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Error::new("would_block", "No live frame available")
            }
            _ => Error::new("transport_error", "Live read failed"),
        })
    }
    pub fn poll_timeout(&mut self, milliseconds: u64) -> Result<()> {
        let stream = match self.inner.get_mut() {
            MaybeTlsStream::Plain(stream) => stream,
            MaybeTlsStream::Rustls(stream) => &mut stream.sock,
            _ => return Err(Error::new("transport_error", "Unsupported TLS backend")),
        };
        stream
            .set_read_timeout(Some(Duration::from_millis(milliseconds.max(1))))
            .map_err(io_error)
    }
    pub fn text(&mut self, frame: &serde_json::Value) -> Result<()> {
        self.inner
            .send(tungstenite::Message::Text(frame.to_string().into()))
            .map_err(|_| Error::new("transport_error", "Live write failed"))
    }
    pub fn binary(&mut self, bytes: &[u8]) -> Result<()> {
        self.inner
            .send(tungstenite::Message::Binary(bytes.to_vec().into()))
            .map_err(|_| Error::new("transport_error", "Live audio write failed"))
    }
    pub fn close(&mut self) -> Result<()> {
        self.inner
            .close(None)
            .map_err(|_| Error::new("transport_error", "Live close failed"))
    }
}
fn io_error(_: std::io::Error) -> Error {
    Error::new("transport_error", "Socket configuration failed")
}
