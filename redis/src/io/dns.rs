use crate::{
    aio::Runtime,
    errors::{ErrorKind, RedisError},
    types::{RedisFuture, RedisResult},
};
use futures_util::FutureExt;
use std::net::SocketAddr;

/// An async DNS resolver for resolving redis domains.
pub trait AsyncDNSResolver: Send + Sync + 'static {
    /// Resolves the host and port to a list of `SocketAddr`.
    fn resolve<'a, 'b: 'a>(
        &'a self,
        host: &'b str,
        port: u16,
    ) -> crate::RedisFuture<'a, Box<dyn Iterator<Item = SocketAddr> + Send + 'a>>;
}

/// Controls how async TCP connection attempts use addresses returned by DNS resolution.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum AsyncConnectionAddrSelection {
    /// Race connection attempts to all resolved socket addresses and use the first
    /// successful connection.
    #[default]
    Race,
    /// Try resolved socket addresses in iterator order until one connects.
    Sequential,
}

/// Default DNS resolver which uses the system's DNS resolver.
#[derive(Clone)]
pub(crate) struct DefaultAsyncDNSResolver;

impl AsyncDNSResolver for DefaultAsyncDNSResolver {
    fn resolve<'a, 'b: 'a>(
        &'a self,
        host: &'b str,
        port: u16,
    ) -> RedisFuture<'a, Box<dyn Iterator<Item = SocketAddr> + Send + 'a>> {
        Box::pin(get_socket_addrs(host, port).map(|vec| {
            Ok(Box::new(vec?.into_iter()) as Box<dyn Iterator<Item = SocketAddr> + Send>)
        }))
    }
}

async fn get_socket_addrs(host: &str, port: u16) -> RedisResult<Vec<SocketAddr>> {
    let socket_addrs: Vec<_> = match Runtime::locate() {
        #[cfg(feature = "tokio-comp")]
        Runtime::Tokio => ::tokio::net::lookup_host((host, port))
            .await
            .map_err(RedisError::from)
            .map(|iter| iter.collect()),

        #[cfg(feature = "smol-comp")]
        Runtime::Smol => ::smol::net::resolve((host, port))
            .await
            .map_err(RedisError::from),
    }?;

    if socket_addrs.is_empty() {
        Err(RedisError::from((
            ErrorKind::InvalidClientConfig,
            "No address found for host",
        )))
    } else {
        Ok(socket_addrs)
    }
}
