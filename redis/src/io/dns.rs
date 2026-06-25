#[cfg(feature = "aio")]
use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

/// An async DNS resovler for resolving redis domain.
#[cfg(feature = "aio")]
pub trait AsyncDNSResolver: Send + Sync + 'static {
    /// Resolves the host and port to a list of `SocketAddr`.
    fn resolve<'a, 'b: 'a>(
        &'a self,
        host: &'b str,
        port: u16,
    ) -> crate::RedisFuture<'a, Box<dyn Iterator<Item = SocketAddr> + Send + 'a>>;
}

/// An async DNS resolver that rotates through a fixed set of IP addresses.
///
/// This resolver is useful when an application resolves a hostname externally
/// and wants Redis connection attempts to spread across the resulting IPs. Each
/// call to [`AsyncDNSResolver::resolve`] returns all configured IPs, rotated so
/// a different IP appears first. Pair it with
/// [`crate::AsyncConnectionAddrSelection::Sequential`] to make redis-rs honor
/// that order while still falling back to later IPs if a connection attempt
/// fails.
#[cfg(feature = "aio")]
#[derive(Clone, Debug)]
pub struct RoundRobinDnsResolver {
    ips: Arc<[IpAddr]>,
    next_index: Arc<AtomicUsize>,
}

#[cfg(feature = "aio")]
impl RoundRobinDnsResolver {
    /// Create a resolver from pre-resolved IP addresses.
    pub fn new(ips: impl IntoIterator<Item = IpAddr>) -> crate::RedisResult<Self> {
        let ips = ips.into_iter().collect::<Vec<_>>().into_boxed_slice();
        if ips.is_empty() {
            return Err(crate::RedisError::from((
                crate::ErrorKind::InvalidClientConfig,
                "RoundRobinDnsResolver requires at least one IP address",
            )));
        }

        Ok(Self {
            ips: ips.into(),
            next_index: Arc::new(AtomicUsize::new(0)),
        })
    }

    fn rotated_socket_addrs(&self, port: u16) -> Vec<SocketAddr> {
        let start = self.next_index.fetch_add(1, Ordering::Relaxed);
        (0..self.ips.len())
            .map(|offset| SocketAddr::new(self.ips[(start + offset) % self.ips.len()], port))
            .collect()
    }
}

#[cfg(feature = "aio")]
impl AsyncDNSResolver for RoundRobinDnsResolver {
    fn resolve<'a, 'b: 'a>(
        &'a self,
        _host: &'b str,
        port: u16,
    ) -> crate::RedisFuture<'a, Box<dyn Iterator<Item = SocketAddr> + Send + 'a>> {
        Box::pin(async move {
            Ok(Box::new(self.rotated_socket_addrs(port).into_iter())
                as Box<dyn Iterator<Item = SocketAddr> + Send>)
        })
    }
}

#[cfg(all(test, feature = "aio"))]
mod tests {
    use super::*;

    #[test]
    fn round_robin_dns_resolver_rejects_empty_ips() {
        assert!(RoundRobinDnsResolver::new([]).is_err());
    }

    #[test]
    fn round_robin_dns_resolver_rotates_first_address() {
        let ip1: IpAddr = "127.0.0.1".parse().expect("valid IP");
        let ip2: IpAddr = "127.0.0.2".parse().expect("valid IP");
        let resolver = RoundRobinDnsResolver::new([ip1, ip2]).expect("valid resolver");

        assert_eq!(
            resolver.rotated_socket_addrs(6379),
            vec![SocketAddr::new(ip1, 6379), SocketAddr::new(ip2, 6379)]
        );
        assert_eq!(
            resolver.rotated_socket_addrs(6379),
            vec![SocketAddr::new(ip2, 6379), SocketAddr::new(ip1, 6379)]
        );
    }
}
