use std::collections::HashMap;
use std::hash::{BuildHasher, Hash};
use std::sync::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use dnser_proto::{Message, Question, RData, Rcode, ResourceRecord};

const NUM_SHARDS: usize = 64;

#[derive(Hash, PartialEq, Eq, Clone)]
struct CacheKey {
    name: Box<str>,
    qtype: u16,
    qclass: u16,
}

impl CacheKey {
    fn from_question(q: &Question) -> Self {
        Self {
            name: q.name.to_ascii_lowercase().into_boxed_str(),
            qtype: q.qtype as u16,
            qclass: u16::from(q.qclass),
        }
    }
}

struct CacheEntry {
    response: Message,
    inserted_at: Instant,
    expires_at: Instant,
}

type Shard = RwLock<HashMap<CacheKey, CacheEntry>>;

pub struct Cache {
    shards: Box<[Shard]>,
    build_hasher: std::collections::hash_map::RandomState,
    capacity: usize,
    len: AtomicUsize,
}

impl Cache {
    pub fn new(capacity: usize) -> Self {
        let per_shard_hint = (capacity / NUM_SHARDS).max(1);
        let shards = (0..NUM_SHARDS)
            .map(|_| RwLock::new(HashMap::with_capacity(per_shard_hint)))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            shards,
            build_hasher: std::collections::hash_map::RandomState::new(),
            capacity,
            len: AtomicUsize::new(0),
        }
    }

    fn shard_index(&self, key: &CacheKey) -> usize {
        self.build_hasher.hash_one(key) as usize & (NUM_SHARDS - 1)
    }

    /// Returns a TTL-rewritten clone of the cached response, or `None` on miss/expiry.
    ///
    /// The shard read lock is released before TTL rewriting so concurrent readers on
    /// other keys are not blocked by the clone work.
    pub fn get(&self, question: &Question) -> Option<Message> {
        let key = CacheKey::from_question(question);
        let shard = self.shards[self.shard_index(&key)].read().unwrap();
        let entry = shard.get(&key)?;

        let now = Instant::now();
        if now >= entry.expires_at {
            return None;
        }

        let elapsed_secs = now
            .duration_since(entry.inserted_at)
            .as_secs()
            .min(u64::from(u32::MAX)) as u32;
        let mut response = entry.response.clone();
        drop(shard); // release read lock before doing any work

        rewrite_ttls(&mut response.answers, elapsed_secs);
        rewrite_ttls(&mut response.authority, elapsed_secs);
        rewrite_ttls(&mut response.additional, elapsed_secs);

        Some(response)
    }

    /// Caches `response` if it is cacheable (NOERROR or NXDOMAIN with a positive TTL).
    ///
    /// Responses with zero TTL, truncated bit set, or non-cacheable rcodes (SERVFAIL,
    /// REFUSED, …) are silently dropped.
    pub fn insert(&self, response: &Message) {
        let [question] = response.questions.as_slice() else {
            return;
        };

        if response.header.is_truncated() {
            return;
        }

        let rcode = match response.header.rcode() {
            Ok(r) => r,
            Err(_) => return,
        };

        let effective_ttl = match rcode {
            Rcode::NoError => {
                if response.answers.is_empty() {
                    // NODATA: use SOA minimum from authority section (RFC 2308 §3).
                    soa_negative_ttl(&response.authority)
                } else {
                    response.answers.iter().map(|r| r.ttl).min()
                }
            }
            Rcode::NXDomain => soa_negative_ttl(&response.authority),
            _ => return,
        };

        let ttl = match effective_ttl {
            Some(0) | None => return,
            Some(t) => t,
        };

        let key = CacheKey::from_question(question);
        let shard_i = self.shard_index(&key);

        // Global capacity enforcement. We never hold two shard locks simultaneously,
        // so there is no deadlock risk.
        if self.len.load(Ordering::Relaxed) >= self.capacity {
            let now = Instant::now();
            let mut evicted = {
                let mut shard = self.shards[shard_i].write().unwrap();
                evict_one(&mut shard, now)
            };
            if !evicted {
                // Target shard was empty; scan other shards for something to evict.
                for (i, other) in self.shards.iter().enumerate() {
                    if i == shard_i {
                        continue;
                    }
                    let mut s = other.write().unwrap();
                    if evict_one(&mut s, now) {
                        evicted = true;
                        break;
                    }
                }
            }
            if evicted {
                self.len.fetch_sub(1, Ordering::Relaxed);
            }
        }

        let now = Instant::now();
        let mut shard = self.shards[shard_i].write().unwrap();
        let is_new = shard
            .insert(
                key,
                CacheEntry {
                    response: response.clone(),
                    inserted_at: now,
                    expires_at: now + Duration::from_secs(u64::from(ttl)),
                },
            )
            .is_none();
        drop(shard);

        if is_new {
            self.len.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Removes all entries whose TTL has elapsed. Called periodically by the server reaper task.
    pub fn evict_expired(&self) {
        let now = Instant::now();
        for shard in self.shards.iter() {
            let mut s = shard.write().unwrap();
            let before = s.len();
            s.retain(|_, v| now < v.expires_at);
            let removed = before - s.len();
            self.len.fetch_sub(removed, Ordering::Relaxed);
        }
    }

    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    pub fn is_empty(&self) -> bool {
        self.len.load(Ordering::Relaxed) == 0
    }
}

/// Evicts one entry from an already-write-locked shard: expired first, then arbitrary.
/// Returns true if an entry was removed.
fn evict_one(shard: &mut HashMap<CacheKey, CacheEntry>, now: Instant) -> bool {
    let mut first: Option<CacheKey> = None;
    let expired = shard.iter().find_map(|(k, e)| {
        if now >= e.expires_at {
            Some(k.clone())
        } else {
            if first.is_none() {
                first = Some(k.clone());
            }
            None
        }
    });
    if let Some(k) = expired.or(first) {
        shard.remove(&k);
        return true;
    }
    false
}

fn rewrite_ttls(records: &mut [ResourceRecord], elapsed_secs: u32) {
    for rr in records {
        rr.ttl = rr.ttl.saturating_sub(elapsed_secs);
    }
}

/// Returns `min(SOA TTL, SOA MINIMUM)` per RFC 2308 §5.
fn soa_negative_ttl(authority: &[ResourceRecord]) -> Option<u32> {
    authority.iter().find_map(|rr| {
        if let RData::SOA { minimum, .. } = &rr.rdata {
            Some(rr.ttl.min(*minimum))
        } else {
            None
        }
    })
}

// Test-only helpers that expose internal state without making fields public.
#[cfg(test)]
impl Cache {
    fn backdate_inserted_at(&self, question: &Question, by: Duration) {
        let key = CacheKey::from_question(question);
        let mut shard = self.shards[self.shard_index(&key)].write().unwrap();
        if let Some(e) = shard.get_mut(&key) {
            e.inserted_at -= by;
        }
    }

    fn force_expire(&self, question: &Question) {
        let key = CacheKey::from_question(question);
        let mut shard = self.shards[self.shard_index(&key)].write().unwrap();
        if let Some(e) = shard.get_mut(&key) {
            e.expires_at = Instant::now() - Duration::from_secs(1);
        }
    }

    fn stored_ttl_secs(&self, question: &Question) -> Option<u64> {
        let key = CacheKey::from_question(question);
        let shard = self.shards[self.shard_index(&key)].read().unwrap();
        shard
            .get(&key)
            .map(|e| e.expires_at.duration_since(e.inserted_at).as_secs())
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::time::Duration;

    use dnser_proto::{Class, Header, Message, Question, RData, Rcode, RecordType, ResourceRecord};

    use super::*;

    fn question() -> Question {
        Question {
            name: "example.com".to_string(),
            qtype: RecordType::A,
            qclass: Class::IN,
        }
    }

    fn a_record(ttl: u32) -> ResourceRecord {
        ResourceRecord {
            name: "example.com".to_string(),
            class: Class::IN,
            ttl,
            rdata: RData::A(Ipv4Addr::new(93, 184, 216, 34)),
        }
    }

    fn noerror_response(ttl: u32) -> Message {
        Message {
            header: Header {
                id: 1,
                flags: Header::QR | Header::RD | Header::RA,
                qd_count: 1,
                an_count: 1,
                ..Default::default()
            },
            questions: vec![question()],
            answers: vec![a_record(ttl)],
            ..Default::default()
        }
    }

    fn servfail_response() -> Message {
        Message {
            header: Header {
                id: 1,
                flags: Header::QR | (Rcode::ServFail as u16),
                qd_count: 1,
                ..Default::default()
            },
            questions: vec![question()],
            ..Default::default()
        }
    }

    fn truncated_response() -> Message {
        Message {
            header: Header {
                id: 1,
                flags: Header::QR | Header::TC | Header::RD,
                qd_count: 1,
                an_count: 1,
                ..Default::default()
            },
            questions: vec![question()],
            answers: vec![a_record(300)],
            ..Default::default()
        }
    }

    fn soa_record(soa_ttl: u32, minimum: u32) -> ResourceRecord {
        ResourceRecord {
            name: "example.com".to_string(),
            class: Class::IN,
            ttl: soa_ttl,
            rdata: RData::SOA {
                mname: "ns1.example.com".to_string(),
                rname: "admin.example.com".to_string(),
                serial: 1,
                refresh: 3600,
                retry: 600,
                expire: 86400,
                minimum,
            },
        }
    }

    fn nxdomain_response(soa_ttl: u32, minimum: u32) -> Message {
        Message {
            header: Header {
                id: 1,
                flags: Header::QR | (Rcode::NXDomain as u16),
                qd_count: 1,
                ns_count: 1,
                ..Default::default()
            },
            questions: vec![question()],
            authority: vec![soa_record(soa_ttl, minimum)],
            ..Default::default()
        }
    }

    #[test]
    fn hit_after_insert() {
        let cache = Cache::new(100);
        cache.insert(&noerror_response(300));
        assert!(cache.get(&question()).is_some());
    }

    #[test]
    fn ttls_are_rewritten_on_hit() {
        let cache = Cache::new(100);
        cache.insert(&noerror_response(300));
        cache.backdate_inserted_at(&question(), Duration::from_secs(100));
        let hit = cache.get(&question()).unwrap();
        assert_eq!(hit.answers[0].ttl, 200);
    }

    #[test]
    fn expired_entry_returns_none() {
        let cache = Cache::new(100);
        cache.insert(&noerror_response(60));
        cache.force_expire(&question());
        assert!(cache.get(&question()).is_none());
    }

    #[test]
    fn zero_ttl_not_cached() {
        let cache = Cache::new(100);
        cache.insert(&noerror_response(0));
        assert!(cache.is_empty());
    }

    #[test]
    fn servfail_not_cached() {
        let cache = Cache::new(100);
        cache.insert(&servfail_response());
        assert!(cache.is_empty());
    }

    #[test]
    fn truncated_not_cached() {
        let cache = Cache::new(100);
        cache.insert(&truncated_response());
        assert!(cache.is_empty());
    }

    #[test]
    fn nxdomain_cached_with_soa_minimum() {
        // SOA TTL=300, SOA minimum=60 → effective TTL = min(300,60) = 60.
        let cache = Cache::new(100);
        cache.insert(&nxdomain_response(300, 60));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.stored_ttl_secs(&question()), Some(60));
    }

    #[test]
    fn evict_expired_clears_stale_entries() {
        let cache = Cache::new(100);
        cache.insert(&noerror_response(300));
        cache.force_expire(&question());
        cache.evict_expired();
        assert!(cache.is_empty());
    }

    #[test]
    fn case_insensitive_lookup() {
        let cache = Cache::new(100);
        cache.insert(&noerror_response(300));
        let q = Question {
            name: "EXAMPLE.COM".to_string(),
            qtype: RecordType::A,
            qclass: Class::IN,
        };
        assert!(cache.get(&q).is_some());
    }

    #[test]
    fn capacity_enforced() {
        let cache = Cache::new(2);
        let base = noerror_response(300);

        cache.insert(&base);
        assert_eq!(cache.len(), 1);

        let mut r2 = base.clone();
        r2.questions[0].name = "other.com".to_string();
        r2.answers[0].name = "other.com".to_string();
        cache.insert(&r2);
        assert_eq!(cache.len(), 2);

        let mut r3 = base.clone();
        r3.questions[0].name = "third.com".to_string();
        r3.answers[0].name = "third.com".to_string();
        cache.insert(&r3);
        assert_eq!(cache.len(), 2);
    }
}
