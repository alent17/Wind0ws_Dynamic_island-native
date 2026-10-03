//! Bounded discovery of a Chromium DevTools endpoint on IPv4 loopback.

use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    num::NonZeroU16,
    time::Duration,
};
use tungstenite::{
    client::{client_with_config, IntoClientRequest},
    handshake::HandshakeError,
    protocol::WebSocketConfig,
    Message, WebSocket,
};

const LOOPBACK: Ipv4Addr = Ipv4Addr::LOCALHOST;
const DEFAULT_REMOTE_DEBUGGING_PORT: u16 = 9223;
const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_BODY_BYTES: usize = 32 * 1024;
const MAX_COMMAND_BYTES: usize = 16 * 1024;
const MAX_UNMATCHED_MESSAGES: usize = 16;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const IO_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CdpVersion {
    pub browser: String,
    pub protocol_version: String,
    pub websocket_debugger_url: String,
}

#[derive(Debug)]
pub enum CdpError {
    InvalidPort,
    Io(std::io::Error),
    InvalidResponse,
    UnsafeWebSocketEndpoint,
    InvalidCommand,
    TimedOut,
    Protocol,
    WebSocket(tungstenite::Error),
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPort => formatter.write_str("CDP 端口无效"),
            Self::Io(_) => formatter.write_str("本机 CDP 服务不可用"),
            Self::InvalidResponse => formatter.write_str("本机 CDP 响应无效"),
            Self::UnsafeWebSocketEndpoint => formatter.write_str("CDP 返回了非本机 WebSocket 地址"),
            Self::InvalidCommand => formatter.write_str("CDP 命令无效或超出长度限制"),
            Self::TimedOut => formatter.write_str("本机 CDP 命令超时"),
            Self::Protocol => formatter.write_str("本机 CDP 返回了错误响应"),
            Self::WebSocket(_) => formatter.write_str("本机 CDP WebSocket 连接失败"),
        }
    }
}

impl std::error::Error for CdpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::WebSocket(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CdpError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<tungstenite::Error> for CdpError {
    fn from(error: tungstenite::Error) -> Self {
        if matches!(
            &error,
            tungstenite::Error::Io(io_error)
                if matches!(io_error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock)
        ) {
            Self::TimedOut
        } else {
            Self::WebSocket(error)
        }
    }
}

/// Connects only to `127.0.0.1`; a remote debugging port never changes host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalCdpClient {
    port: NonZeroU16,
}

impl Default for LocalCdpClient {
    fn default() -> Self {
        Self::new(DEFAULT_REMOTE_DEBUGGING_PORT).expect("default CDP port is nonzero")
    }
}

impl LocalCdpClient {
    pub fn new(port: u16) -> Result<Self, CdpError> {
        Ok(Self {
            port: NonZeroU16::new(port).ok_or(CdpError::InvalidPort)?,
        })
    }

    pub const fn port(self) -> u16 {
        self.port.get()
    }

    pub fn version(self) -> Result<CdpVersion, CdpError> {
        let address = SocketAddr::V4(SocketAddrV4::new(LOOPBACK, self.port()));
        let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        write!(
            stream,
            "GET /json/version HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nAccept: application/json\r\n\r\n",
            self.port()
        )?;

        let mut response = Vec::with_capacity(1024);
        stream
            .take((MAX_HEADER_BYTES + MAX_BODY_BYTES + 1) as u64)
            .read_to_end(&mut response)?;
        let body = parse_http_json_body(&response)?;
        let value: serde_json::Value =
            serde_json::from_slice(body).map_err(|_| CdpError::InvalidResponse)?;
        let browser = required_string(&value, "Browser")?;
        let protocol_version = required_string(&value, "Protocol-Version")?;
        let websocket_debugger_url = required_string(&value, "webSocketDebuggerUrl")?;
        validate_websocket_endpoint(websocket_debugger_url, self.port())?;
        Ok(CdpVersion {
            browser: browser.to_string(),
            protocol_version: protocol_version.to_string(),
            websocket_debugger_url: websocket_debugger_url.to_string(),
        })
    }

    /// Discovers the browser endpoint and opens a bounded CDP WebSocket.
    /// The endpoint is revalidated and the socket connects directly to IPv4
    /// loopback without resolving a host from the browser response.
    pub fn connect(self) -> Result<CdpConnection, CdpError> {
        let version = self.version()?;
        let path = validate_websocket_endpoint(&version.websocket_debugger_url, self.port())?;
        let address = SocketAddr::V4(SocketAddrV4::new(LOOPBACK, self.port()));
        let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        let endpoint = format!("ws://127.0.0.1:{}{path}", self.port());
        let request = endpoint
            .into_client_request()
            .map_err(CdpError::WebSocket)?;
        let config = WebSocketConfig::default()
            .read_buffer_size(4096)
            .write_buffer_size(0)
            .max_write_buffer_size(MAX_COMMAND_BYTES + 1024)
            .max_message_size(Some(MAX_BODY_BYTES))
            .max_frame_size(Some(MAX_BODY_BYTES));
        let (socket, _) =
            client_with_config(request, stream, Some(config)).map_err(|error| match error {
                HandshakeError::Failure(error) => CdpError::from(error),
                HandshakeError::Interrupted(_) => CdpError::Protocol,
            })?;
        Ok(CdpConnection { socket, next_id: 1 })
    }
}

pub struct CdpConnection {
    socket: WebSocket<TcpStream>,
    next_id: u64,
}

impl CdpConnection {
    /// Sends one CDP method and returns its `result`, ignoring unrelated events.
    pub fn request(
        &mut self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, CdpError> {
        if method.is_empty()
            || method.len() > 128
            || !method
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(CdpError::InvalidCommand);
        }
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(CdpError::InvalidCommand)?;
        let request = serde_json::json!({"id":id,"method":method,"params":params});
        let request = serde_json::to_string(&request).map_err(|_| CdpError::InvalidCommand)?;
        if request.len() > MAX_COMMAND_BYTES {
            return Err(CdpError::InvalidCommand);
        }
        self.socket.send(Message::Text(request.into()))?;

        for _ in 0..=MAX_UNMATCHED_MESSAGES {
            let response = self.socket.read()?;
            let Message::Text(response) = response else {
                if response.is_close() {
                    return Err(CdpError::Protocol);
                }
                continue;
            };
            let response: serde_json::Value =
                serde_json::from_str(response.as_str()).map_err(|_| CdpError::InvalidResponse)?;
            if response.get("id").and_then(serde_json::Value::as_u64) != Some(id) {
                continue;
            }
            if response.get("error").is_some() {
                return Err(CdpError::Protocol);
            }
            return response
                .get("result")
                .cloned()
                .ok_or(CdpError::InvalidResponse);
        }
        Err(CdpError::TimedOut)
    }
}

fn required_string<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str, CdpError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 1024)
        .ok_or(CdpError::InvalidResponse)
}

fn parse_http_json_body(response: &[u8]) -> Result<&[u8], CdpError> {
    if response.len() > MAX_HEADER_BYTES + MAX_BODY_BYTES {
        return Err(CdpError::InvalidResponse);
    }
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(CdpError::InvalidResponse)?;
    if header_end > MAX_HEADER_BYTES {
        return Err(CdpError::InvalidResponse);
    }
    let headers =
        std::str::from_utf8(&response[..header_end]).map_err(|_| CdpError::InvalidResponse)?;
    let mut lines = headers.split("\r\n");
    if lines.next() != Some("HTTP/1.1 200 OK") {
        return Err(CdpError::InvalidResponse);
    }
    let mut content_length = None;
    let mut is_json = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(CdpError::InvalidResponse);
        };
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(CdpError::InvalidResponse);
            }
            let parsed = value
                .trim()
                .parse::<usize>()
                .map_err(|_| CdpError::InvalidResponse)?;
            if parsed > MAX_BODY_BYTES {
                return Err(CdpError::InvalidResponse);
            }
            content_length = Some(parsed);
        } else if name.eq_ignore_ascii_case("content-type") {
            is_json = value
                .trim()
                .split(';')
                .next()
                .is_some_and(|mime| mime.eq_ignore_ascii_case("application/json"));
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(CdpError::InvalidResponse);
        }
    }
    let body = &response[header_end + 4..];
    let expected_length = content_length.ok_or(CdpError::InvalidResponse)?;
    if !is_json || body.len() != expected_length {
        return Err(CdpError::InvalidResponse);
    }
    Ok(body)
}

fn validate_websocket_endpoint(endpoint: &str, expected_port: u16) -> Result<&str, CdpError> {
    let prefix = format!("ws://127.0.0.1:{expected_port}/devtools/browser/");
    let Some(id) = endpoint.strip_prefix(&prefix) else {
        return Err(CdpError::UnsafeWebSocketEndpoint);
    };
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(CdpError::UnsafeWebSocketEndpoint);
    }
    Ok(&endpoint[prefix.len() - 1..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn read_request_headers(stream: &mut TcpStream) -> String {
        let mut request = Vec::with_capacity(512);
        while request.len() < 4096 {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") {
                return String::from_utf8(request).unwrap();
            }
        }
        panic!("HTTP request headers exceeded test limit");
    }

    #[test]
    fn rejects_port_zero_and_uses_the_documented_default_port() {
        assert!(matches!(LocalCdpClient::new(0), Err(CdpError::InvalidPort)));
        assert_eq!(LocalCdpClient::default().port(), 9223);
        assert_eq!(LocalCdpClient::new(9230).unwrap().port(), 9230);
    }

    #[test]
    fn accepts_only_browser_websocket_urls_on_configured_ipv4_loopback() {
        assert!(
            validate_websocket_endpoint("ws://127.0.0.1:9223/devtools/browser/abc-123", 9223)
                .is_ok()
        );
        for endpoint in [
            "ws://localhost:9223/devtools/browser/id",
            "ws://127.0.0.2:9223/devtools/browser/id",
            "ws://192.168.1.10:9223/devtools/browser/id",
            "ws://127.0.0.1:9224/devtools/browser/id",
            "ws://user@127.0.0.1:9223/devtools/browser/id",
            "ws://127.0.0.1:9223/devtools/page/id",
            "ws://127.0.0.1:9223/devtools/browser/id?host=evil",
        ] {
            assert!(
                matches!(
                    validate_websocket_endpoint(endpoint, 9223),
                    Err(CdpError::UnsafeWebSocketEndpoint)
                ),
                "endpoint should be rejected: {endpoint}"
            );
        }
    }

    #[test]
    fn version_probe_uses_only_loopback_and_bounds_http_response() {
        let listener = TcpListener::bind(SocketAddrV4::new(LOOPBACK, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, peer) = listener.accept().unwrap();
            assert!(peer.ip().is_loopback());
            let request = read_request_headers(&mut stream);
            assert!(request.starts_with("GET /json/version HTTP/1.1\r\n"));
            assert!(request.contains(&format!("Host: 127.0.0.1:{port}\r\n")));
            let body = format!(
                "{{\"Browser\":\"Chrome/120.0\",\"Protocol-Version\":\"1.3\",\"webSocketDebuggerUrl\":\"ws://127.0.0.1:{port}/devtools/browser/test-id\"}}"
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });

        let version = LocalCdpClient::new(port).unwrap().version().unwrap();
        server.join().unwrap();
        assert_eq!(version.browser, "Chrome/120.0");
        assert_eq!(version.protocol_version, "1.3");
        assert!(version
            .websocket_debugger_url
            .starts_with("ws://127.0.0.1:"));
    }

    #[test]
    fn cdp_requests_match_ids_and_ignore_events_on_loopback_websocket() {
        let listener = TcpListener::bind(SocketAddrV4::new(LOOPBACK, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut http, peer) = listener.accept().unwrap();
            assert!(peer.ip().is_loopback());
            let request = read_request_headers(&mut http);
            assert!(request.starts_with("GET /json/version HTTP/1.1\r\n"));
            let body = format!(
                "{{\"Browser\":\"Chrome/120.0\",\"Protocol-Version\":\"1.3\",\"webSocketDebuggerUrl\":\"ws://127.0.0.1:{port}/devtools/browser/test-id\"}}"
            );
            write!(
                http,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            drop(http);

            let (stream, peer) = listener.accept().unwrap();
            assert!(peer.ip().is_loopback());
            let mut websocket = tungstenite::accept(stream).unwrap();
            let request: serde_json::Value = match websocket.read().unwrap() {
                Message::Text(message) => serde_json::from_str(message.as_str()).unwrap(),
                message => panic!("expected CDP text request, received {message:?}"),
            };
            assert_eq!(request["method"], "Runtime.evaluate");
            assert_eq!(request["params"]["expression"], "1 + 1");
            websocket
                .send(Message::text(
                    r#"{"method":"Runtime.executionContextCreated","params":{}}"#,
                ))
                .unwrap();
            websocket
                .send(Message::text(
                    r#"{"id":1,"result":{"result":{"type":"number","value":2}}}"#,
                ))
                .unwrap();
        });

        let mut connection = LocalCdpClient::new(port).unwrap().connect().unwrap();
        assert!(matches!(
            connection.request("../Runtime.evaluate", &serde_json::json!({})),
            Err(CdpError::InvalidCommand)
        ));
        let result = connection
            .request(
                "Runtime.evaluate",
                &serde_json::json!({"expression":"1 + 1"}),
            )
            .unwrap();
        server.join().unwrap();
        assert_eq!(result["result"]["value"], 2);
    }

    #[test]
    fn refuses_an_external_websocket_target_returned_by_loopback_http() {
        let listener = TcpListener::bind(SocketAddrV4::new(LOOPBACK, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request_headers(&mut stream);
            let body = format!(
                "{{\"Browser\":\"Chrome\",\"Protocol-Version\":\"1.3\",\"webSocketDebuggerUrl\":\"ws://192.168.1.1:{port}/devtools/browser/id\"}}"
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });

        assert!(matches!(
            LocalCdpClient::new(port).unwrap().version(),
            Err(CdpError::UnsafeWebSocketEndpoint)
        ));
        server.join().unwrap();
    }
}
