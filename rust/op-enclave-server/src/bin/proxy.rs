//! HTTP proxy to vsock.
//!
//! This is a small HTTP proxy that forwards requests to a vsock service,
//! matching Go's `cmd/server/main.go`.

use std::io::{Read, Write};
use std::net::SocketAddr;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use op_enclave_server::transport::{DEFAULT_PROXY_PORT, DEFAULT_VSOCK_CID, DEFAULT_VSOCK_PORT};

#[cfg(unix)]
use vsock::{VsockAddr, VsockStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    #[cfg(not(unix))]
    {
        tracing::error!("vsock proxy is only supported on Unix platforms");
        return Err("vsock not supported".into());
    }

    #[cfg(unix)]
    {
        let cid = std::env::var("VSOCK_CID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_VSOCK_CID);

        let vsock_port = std::env::var("VSOCK_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_VSOCK_PORT);

        let http_port: u16 = std::env::var("HTTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_PROXY_PORT);

        let addr = SocketAddr::from(([0, 0, 0, 0], http_port));

        tracing::info!(
            http_port = http_port,
            vsock_cid = cid,
            vsock_port = vsock_port,
            "starting HTTP proxy to vsock"
        );

        let listener = TcpListener::bind(addr).await?;

        loop {
            let (stream, peer_addr) = listener.accept().await?;

            tracing::debug!(peer = %peer_addr, "accepted connection");

            tokio::spawn(async move {
                let io = TokioIo::new(stream);

                let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                    let cid = cid;
                    let vsock_port = vsock_port;
                    async move { handle_request(req, cid, vsock_port).await }
                });

                if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                    tracing::warn!("connection error: {e}");
                }
            });
        }
    }
}

/// Handle an HTTP request by forwarding it to vsock.
#[cfg(unix)]
async fn handle_request(
    req: Request<hyper::body::Incoming>,
    cid: u32,
    port: u32,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    // Only handle POST requests
    if req.method() != Method::POST {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::new()))
            .expect("failed to build response"));
    }

    // Read the request body
    let body = req.collect().await?.to_bytes();

    // Forward to vsock in a blocking task
    let response = tokio::task::spawn_blocking(move || forward_to_vsock(cid, port, &body))
        .await
        .map_err(|e| {
            tracing::error!("spawn_blocking error: {e}");
        })
        .ok()
        .flatten();

    response.map_or_else(
        || {
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::new()))
                .expect("failed to build response"))
        },
        |data| {
            Ok(Response::builder()
                .header("content-type", "application/json")
                .body(Full::new(Bytes::from(data)))
                .expect("failed to build response"))
        },
    )
}

/// Forward a request to vsock and return the response.
#[cfg(unix)]
fn forward_to_vsock(cid: u32, port: u32, request: &[u8]) -> Option<Vec<u8>> {
    // Connect to vsock
    let mut conn = VsockStream::connect(&VsockAddr::new(cid, port)).ok()?;

    // Write the request
    conn.write_all(request).ok()?;

    // Read the response
    let mut response = Vec::new();
    let mut buffer = [0u8; 8192];

    loop {
        match conn.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                response.extend_from_slice(&buffer[..n]);
                // Check if we have a complete JSON response
                if let Ok(s) = std::str::from_utf8(&response) {
                    if serde_json::from_str::<serde_json::Value>(s).is_ok() {
                        break;
                    }
                }
            }
        }
    }

    if response.is_empty() {
        None
    } else {
        Some(response)
    }
}
