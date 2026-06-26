# dnser-cache

Sharded TTL-aware DNS response cache for [dnser](../../). Safe to share across async tasks without external locking.

## What it does

- **Sharded** `RwLock<HashMap>` internally — concurrent reads on different shards don't contend.
- **TTL-aware**: served entries have their TTLs decremented by the time they've sat in the cache; entries past expiry are skipped on lookup and reaped in the background.
- **Negative caching** per RFC 2308: NXDOMAIN and NODATA responses are cached using the authority section's SOA `MINIMUM` field, clamped by a configurable maximum.
- **Bounded** — when the cache is full, one entry is evicted under a single write-lock acquisition to make room.
- **Case-insensitive** on the query name, as DNS requires (RFC 1035 §2.3.3).
- Entries are stored as `Arc<Message>` so a cache hit clones an `Arc` and releases the read lock before deep-cloning the message for the caller.

## Usage

```rust
use dnser_cache::Cache;

let cache = Cache::new(10_000, 3600); // capacity, max negative TTL (secs)

if let Some(response) = cache.get(&question) {
    return response;
}

let response = resolve(&question).await?;
cache.insert(&response);
```

Call `evict_expired()` periodically from a background task to reap stale entries (the lookup path also skips them lazily).
