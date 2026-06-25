# Async DNS routing design notes

These notes capture the API boundary tested by the fork-only DNS routing
experiments.

## Requirement

Some Redis deployments expose one hostname backed by multiple A or AAAA records.
Applications may want each new Redis connection to start with a deliberately
chosen address from that set, for example to spread a connection pool uniformly
across Route53 answers.

The important behavior is connection-start address selection. It is not a
different Redis protocol behavior, and it usually does not require replacing the
TCP or TLS connector.

## What the existing hook can already do

`AsyncDNSResolver` is called once for each async TCP or TLS connection attempt.
It receives the Redis URL host and port and returns an iterator of `SocketAddr`s.
That is enough for an application resolver to:

- resolve the hostname with another DNS implementation such as Hickory DNS;
- cache or refresh the full answer set using application policy;
- select one address per connection attempt; and
- return only that selected address to force deterministic dialing with today's
  redis-rs API.

That means a uniform pool warmup can be implemented outside redis-rs today if
the application is comfortable returning one address per `resolve` call and
letting pool-level retry create the next attempt.

## Missing behavior for ordered fallback

The current async TCP/TLS connector races every address returned by
`AsyncDNSResolver` with `select_ok`. That is good default behavior for ordinary
DNS resolution, but it means the resolver cannot express "try this selected
address first, then fall back to the remaining addresses in this order".

The `codex/async-addr-selection` experiment adds an address selection policy to
`AsyncConnectionConfig`:

- `Race`, the current default;
- `Sequential`, which dials returned addresses in iterator order.

With that option, an application resolver can return the full rotated DNS answer
set. redis-rs will dial the chosen first address first and still preserve
fallback within the same connection attempt.

## Optional redis-rs helper

The `codex/round-robin-dns-resolver` experiment adds a small
`RoundRobinDnsResolver` for callers that already have a fixed pre-resolved IP
set. It is not required for the Route53 case if the application owns DNS refresh
policy, but it demonstrates the intended contract:

1. resolver returns all addresses in selected order;
2. `AsyncConnectionAddrSelection::Sequential` honors that order;
3. connection pooling can create connections concurrently without a sleep-based
   DNS warmup.

## Limitations that remain

- `Client::get_async_pubsub` and `Client::get_async_monitor` do not accept
  `AsyncConnectionConfig`, so they still use the default async DNS resolver.
- A resolver only chooses socket addresses. It does not observe per-address
  connection timing, TLS handshake failures, or Redis authentication results.
- The hook receives only host and port. It intentionally does not own TLS SNI,
  `TcpSettings`, runtime selection, or socket construction.
- Cluster async has its own builder-level resolver hook. Any address selection
  policy should be checked separately for cluster connection creation paths.

## Why a full connector hook is larger

A full async connector abstraction would need to replace more than DNS. It would
need to own or delegate:

- Tokio and smol runtime-specific socket creation;
- TCP settings;
- TLS feature combinations and SNI handling;
- Unix socket behavior or a non-TCP escape hatch;
- connection timeout and cancellation semantics;
- PubSub, Monitor, standalone, manager, and cluster call sites; and
- error reporting that remains compatible with existing `RedisError` behavior.

That shape is only justified if applications need to control transport creation
itself, not just address ordering. For the Route53 pool distribution problem,
the smaller API surface is to keep DNS resolution pluggable and add ordered
address selection.

## Recommended split

1. Keep application-specific Hickory DNS lookup and refresh policy outside
   redis-rs.
2. Add ordered address selection to redis-rs if same-attempt fallback across the
   rotated answer set is required.
3. Add a redis-rs resolver helper only if there is broad value in a fixed-IP
   round-robin utility.
4. Avoid a full connector hook unless a separate use case requires replacing
   TCP/TLS dialing.
