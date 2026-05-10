//! Tiny TTL + explicit-invalidation cache used by the per-key history
//! lookups (`thread_metadata_json`, `get_skill_variant`,
//! `latest_goal_run_for_thread`).
//!
//! Design notes
//! ============
//! * Each entry has a TTL: even if a write site forgets to invalidate, the
//!   value becomes fresh again at most after `ttl`. This is the safety
//!   net.
//! * Write paths call `invalidate(key)` to drop a stale entry immediately
//!   — that's the primary correctness mechanism. The TTL exists because
//!   tracking *every* possible write surface is fragile (especially for
//!   goal_runs, which can be touched via a half-dozen different code
//!   paths).
//! * Bounded by `max_entries` to prevent unbounded growth on systems with
//!   millions of unique keys. When full, the cache simply drops the
//!   incoming insert; we never want a cache miss to *also* pay an
//!   eviction-bookkeeping cost on a hot path.
//! * Lock granularity: a single `Mutex` over the whole `HashMap`. The
//!   queries this caches return `Option<Small>` types, lookup is microsec,
//!   and contention is not a real concern.
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct CacheEntry<V> {
    value: V,
    inserted_at: Instant,
}

#[derive(Debug)]
pub(super) struct TtlCache<K, V> {
    inner: Mutex<HashMap<K, CacheEntry<V>>>,
    ttl: Duration,
    max_entries: usize,
}

impl<K, V> TtlCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    pub(super) fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::with_capacity(max_entries.min(64))),
            ttl,
            max_entries,
        }
    }

    /// Look up a key; returns `None` if missing OR if the entry has
    /// exceeded the TTL (in which case the stale entry is removed).
    pub(super) fn get(&self, key: &K) -> Option<V> {
        let mut guard = self.inner.lock().ok()?;
        let stale = match guard.get(key) {
            Some(entry) => entry.inserted_at.elapsed() > self.ttl,
            None => return None,
        };
        if stale {
            guard.remove(key);
            return None;
        }
        guard.get(key).map(|entry| entry.value.clone())
    }

    /// Insert a (key, value). If the cache is at `max_entries`, the
    /// insert is dropped silently — better to repeat a cheap query than
    /// to evict an entry that might be hot.
    pub(super) fn insert(&self, key: K, value: V) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        if guard.len() >= self.max_entries && !guard.contains_key(&key) {
            return;
        }
        guard.insert(
            key,
            CacheEntry {
                value,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Drop a single key immediately. Called by write paths on a known
    /// mutation.
    pub(super) fn invalidate(&self, key: &K) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.remove(key);
        }
    }

    /// Drop every entry. Used by writes whose effect spans multiple
    /// keys (e.g., a batched upsert that touches an unbounded set of
    /// thread_ids — cheaper to flush than to enumerate).
    pub(super) fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.clear();
        }
    }
}
