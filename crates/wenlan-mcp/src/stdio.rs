//! Stdio compatibility for clients that probe modern discovery before legacy initialization.
//!
//! `rmcp` 1.5 expects `initialize` to be the first request. Some clients send
//! `server/discover` first, which is an optional method for an older server. A
//! method-not-found response is the protocol's compatibility signal; the
//! transport must remain open so the client can continue with `initialize`.

use rmcp::{
    model::{ClientJsonRpcMessage, ClientRequest, ErrorCode, ServerJsonRpcMessage},
    service::{RoleServer, RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{async_rw::AsyncRwTransport, Transport},
    ErrorData,
};

const SERVER_DISCOVER_METHOD: &str = "server/discover";

/// A server transport that declines an optional pre-initialize discovery probe.
pub struct DiscoveryFallback<T> {
    inner: T,
    initialized: bool,
}

impl<T> DiscoveryFallback<T> {
    /// Wrap an existing server transport with legacy discovery compatibility.
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            initialized: false,
        }
    }
}

impl<T> Transport<RoleServer> for DiscoveryFallback<T>
where
    T: Transport<RoleServer>,
{
    type Error = T::Error;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.inner.send(item)
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        loop {
            let message = self.inner.receive().await?;

            if !self.initialized {
                if let ClientJsonRpcMessage::Request(request) = &message {
                    match &request.request {
                        ClientRequest::CustomRequest(custom)
                            if custom.method == SERVER_DISCOVER_METHOD =>
                        {
                            let response = ServerJsonRpcMessage::error(
                                ErrorData::new(
                                    ErrorCode::METHOD_NOT_FOUND,
                                    "Method not found",
                                    None,
                                ),
                                request.id.clone(),
                            );
                            if self.inner.send(response).await.is_err() {
                                return None;
                            }
                            continue;
                        }
                        ClientRequest::InitializeRequest(_) => {
                            self.initialized = true;
                        }
                        _ => {}
                    }
                }
            }

            return Some(message);
        }
    }

    fn close(&mut self) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
        self.inner.close()
    }
}

/// Construct the Wenlan stdio transport with pre-initialize discovery fallback.
pub fn stdio(
) -> DiscoveryFallback<AsyncRwTransport<RoleServer, tokio::io::Stdin, tokio::io::Stdout>> {
    let (stdin, stdout) = rmcp::transport::stdio();
    DiscoveryFallback::new(AsyncRwTransport::new_server(stdin, stdout))
}
