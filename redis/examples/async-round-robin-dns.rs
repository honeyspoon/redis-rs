use redis::{
    AsyncConnectionAddrSelection, AsyncConnectionConfig, Client, RedisResult,
    io::RoundRobinDnsResolver,
};
use std::net::IpAddr;

#[tokio::main]
async fn main() -> RedisResult<()> {
    let ips: Vec<IpAddr> = ["127.0.0.1"]
        .into_iter()
        .map(|ip| ip.parse().expect("valid IP address"))
        .collect();

    let config = AsyncConnectionConfig::new()
        .set_dns_resolver(RoundRobinDnsResolver::new(ips)?)
        .set_connection_addr_selection(AsyncConnectionAddrSelection::Sequential);

    let client = Client::open("redis://example.invalid:6379/")?;
    let mut connection = client
        .get_multiplexed_async_connection_with_config(&config)
        .await?;

    redis::cmd("PING")
        .query_async::<String>(&mut connection)
        .await?;

    Ok(())
}
