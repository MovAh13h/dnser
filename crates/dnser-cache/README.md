# dnser-cache

TTL-aware concurrent DNS record cache. Entries expire automatically based on the TTL from the original record, and the cache is safe to share across async tasks without external locking. Tracks negative responses (NXDOMAIN) separately to avoid hammering upstream for known-missing names.
