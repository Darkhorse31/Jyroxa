//! Segment maths and the runtime work-stealing segment table.
//!
//! Downloads are split into byte ranges (inclusive `start..=end`). At runtime
//! each segment tracks its own cursor with atomics. Workers first drain a queue
//! of pending segments; once the queue is empty an idle worker *steals* work by
//! splitting the largest still-pending segment in half and taking the second
//! half. That keeps every connection busy right to the end of the file and
//! smooths over slow mirrors or unlucky segments.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::state::SegmentState;

/// Split a file of `total` bytes into `n` contiguous inclusive ranges.
///
/// The final segment absorbs any remainder. Returns fewer than `n` segments if
/// `total` is smaller than `n`.
pub fn split_initial(total: u64, n: usize) -> Vec<(u64, u64)> {
    if total == 0 {
        return vec![];
    }
    let n = (n.max(1) as u64).min(total); // never create empty segments
    let base = total / n;
    let mut ranges = Vec::with_capacity(n as usize);
    let mut start = 0u64;
    for i in 0..n {
        let len = if i == n - 1 { total - start } else { base };
        let end = start + len - 1;
        ranges.push((start, end));
        start = end + 1;
    }
    ranges
}

/// A single runtime segment. `end` is mutable (atomic) so it can shrink when
/// another worker steals the tail.
#[derive(Debug)]
pub struct Segment {
    pub index: usize,
    pub start: u64,
    /// Inclusive end offset; may be lowered by a steal.
    pub end: AtomicU64,
    /// Absolute offset of the next byte to download.
    pub current: AtomicU64,
    /// Set when the segment is fully downloaded.
    pub done: AtomicBool,
}

impl Segment {
    pub fn new(index: usize, start: u64, end: u64, current: u64) -> Arc<Self> {
        Arc::new(Segment {
            index,
            start,
            end: AtomicU64::new(end),
            current: AtomicU64::new(current),
            done: AtomicBool::new(current > end),
        })
    }

    pub fn end(&self) -> u64 {
        self.end.load(Ordering::Acquire)
    }
    pub fn current(&self) -> u64 {
        self.current.load(Ordering::Acquire)
    }
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }
    pub fn mark_done(&self) {
        self.done.store(true, Ordering::Release);
    }

    /// Bytes still to download for this segment.
    pub fn remaining(&self) -> u64 {
        if self.is_done() {
            return 0;
        }
        let end = self.end();
        let cur = self.current();
        if cur > end {
            0
        } else {
            end - cur + 1
        }
    }

    pub fn snapshot(&self) -> SegmentState {
        let cur = self.current();
        SegmentState {
            index: self.index,
            start: self.start,
            end: self.end(),
            downloaded: cur.saturating_sub(self.start),
        }
    }
}

/// Shared table of runtime segments with a pending queue and a work-stealing
/// allocator.
pub struct SegmentTable {
    /// Every segment ever created, for snapshotting and stealing.
    all: Mutex<Vec<Arc<Segment>>>,
    /// Segments not yet handed to a worker.
    queue: Mutex<VecDeque<Arc<Segment>>>,
    next_index: AtomicU64,
    /// Minimum size of each half for a split to be worthwhile.
    min_steal: u64,
    /// Cap segment splits to avoid handshake storms on lossy links.
    steal_cap: usize,
}

impl SegmentTable {
    /// Build a table from inclusive ranges (fresh download).
    pub fn from_ranges(ranges: &[(u64, u64)], min_steal: u64, steal_cap: usize) -> Self {
        let segs: Vec<Arc<Segment>> = ranges
            .iter()
            .enumerate()
            .map(|(i, &(s, e))| Segment::new(i, s, e, s))
            .collect();
        Self::build(segs, min_steal, steal_cap)
    }

    /// Build a table from persisted state (resume). Completed segments are kept
    /// for the snapshot but are not re-queued.
    pub fn from_state(segments: &[SegmentState], min_steal: u64, steal_cap: usize) -> Self {
        let segs: Vec<Arc<Segment>> = segments
            .iter()
            .map(|s| Segment::new(s.index, s.start, s.end, s.cursor()))
            .collect();
        Self::build(segs, min_steal, steal_cap)
    }

    fn build(segs: Vec<Arc<Segment>>, min_steal: u64, steal_cap: usize) -> Self {
        let queue: VecDeque<Arc<Segment>> =
            segs.iter().filter(|s| s.remaining() > 0).cloned().collect();
        let next = segs.iter().map(|s| s.index as u64 + 1).max().unwrap_or(0);
        let cap = steal_cap.max(segs.len()).max(8);
        SegmentTable {
            all: Mutex::new(segs),
            queue: Mutex::new(queue),
            next_index: AtomicU64::new(next),
            min_steal: min_steal.max(1),
            steal_cap: cap,
        }
    }

    /// Number of segments in the table.
    pub fn len(&self) -> usize {
        self.all.lock().unwrap().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of pending (queued) segments waiting for a worker.
    pub fn queued(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    /// A clone of all segments (for tests and inspection).
    pub fn segments(&self) -> Vec<Arc<Segment>> {
        self.all.lock().unwrap().clone()
    }

    /// Whether all segments are fully downloaded.
    pub fn all_done(&self) -> bool {
        let segs = self.all.lock().unwrap();
        !segs.is_empty() && segs.iter().all(|s| s.remaining() == 0)
    }

    /// Snapshot every segment for persistence.
    pub fn snapshot(&self) -> Vec<SegmentState> {
        self.all
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.snapshot())
            .collect()
    }

    /// Put a segment back on the pending queue after a transient failure.
    pub fn requeue(&self, seg: Arc<Segment>) {
        if seg.remaining() > 0 {
            self.queue.lock().unwrap().push_back(seg);
        }
    }

    /// Get the next unit of work for a worker: first from the queue, then by
    /// stealing when `allow_steal` is true (another worker is idle).
    pub fn next_work(&self, allow_steal: bool) -> Option<Arc<Segment>> {
        if let Some(s) = self.queue.lock().unwrap().pop_front() {
            if s.remaining() > 0 {
                return Some(s);
            }
        }
        if allow_steal {
            self.steal()
        } else {
            None
        }
    }

    /// Steal work: prefer the largest remaining slice; tie-break on slowest cursor.
    pub fn steal(&self) -> Option<Arc<Segment>> {
        let min_tail = self.min_steal.saturating_mul(2);
        self.try_steal(min_tail)
            .or_else(|| self.try_steal(self.min_steal.max(64 * 1024)))
    }

    /// Split the tail of a specific in-progress segment after a stall.
    pub fn split_tail(&self, victim: &Arc<Segment>) -> Option<Arc<Segment>> {
        let mut segs = self.all.lock().unwrap();
        if segs.len() >= self.steal_cap {
            return None;
        }
        split_segment_tail(&mut segs, victim, self.min_steal.max(64 * 1024), &self.next_index)
    }

    fn try_steal(&self, min_tail: u64) -> Option<Arc<Segment>> {
        let mut segs = self.all.lock().unwrap();
        if segs.len() >= self.steal_cap {
            return None;
        }

        let mut best: Option<(usize, u64, u64)> = None;
        for (i, s) in segs.iter().enumerate() {
            let rem = s.remaining();
            if rem < min_tail {
                continue;
            }
            let cur = s.current();
            let replace = best
                .map(|(_, r, c)| rem > r || (rem == r && cur < c))
                .unwrap_or(true);
            if replace {
                best = Some((i, rem, cur));
            }
        }

        let (victim_idx, _, _) = best?;
        let victim = segs[victim_idx].clone();
        split_segment_tail(&mut segs, &victim, min_tail, &self.next_index)
    }
}

fn split_segment_tail(
    segs: &mut Vec<Arc<Segment>>,
    victim: &Arc<Segment>,
    min_tail: u64,
    next_index: &AtomicU64,
) -> Option<Arc<Segment>> {
    let cur = victim.current();
    let end = victim.end();
    if cur > end {
        return None;
    }
    let rem = end.saturating_sub(cur).saturating_add(1);
    if rem < min_tail {
        return None;
    }
    let mid = cur + (end - cur) / 2;
    if mid <= cur || mid >= end {
        return None;
    }

    victim.end.store(mid, Ordering::Release);
    let new_index = next_index.fetch_add(1, Ordering::Relaxed) as usize;
    let stolen = Segment::new(new_index, mid + 1, end, mid + 1);
    segs.push(stolen.clone());
    Some(stolen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_even() {
        let r = split_initial(1000, 4);
        assert_eq!(r, vec![(0, 249), (250, 499), (500, 749), (750, 999)]);
    }

    #[test]
    fn split_with_remainder() {
        let r = split_initial(1003, 4);
        assert_eq!(r.first().unwrap().0, 0);
        assert_eq!(r.last().unwrap().1, 1002);
        for w in r.windows(2) {
            assert_eq!(w[0].1 + 1, w[1].0);
        }
        let covered: u64 = r.iter().map(|(s, e)| e - s + 1).sum();
        assert_eq!(covered, 1003);
    }

    #[test]
    fn split_more_parts_than_bytes() {
        assert_eq!(split_initial(3, 8), vec![(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn split_zero() {
        assert!(split_initial(0, 4).is_empty());
    }

    #[test]
    fn queue_then_steal() {
        let table = SegmentTable::from_ranges(&[(0, 999)], 10, 16);
        assert_eq!(table.queued(), 1);
        // First worker claims the queued segment.
        let first = table.next_work(true).expect("queued work");
        assert_eq!(first.start, 0);
        assert_eq!(table.queued(), 0);
        // Second worker must steal the tail of the first.
        let stolen = table.next_work(true).expect("stolen work");
        assert_eq!(stolen.start, 500);
        assert_eq!(stolen.end(), 999);
        assert_eq!(first.end(), 499);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn steal_respects_min() {
        let table = SegmentTable::from_ranges(&[(0, 15)], 100, 16);
        let _ = table.next_work(true);
        assert!(table.steal().is_none());
    }

    #[test]
    fn steal_follows_cursor() {
        let table = SegmentTable::from_ranges(&[(0, 999)], 10, 16);
        let victim = table.next_work(true).unwrap();
        victim.current.store(400, Ordering::Release);
        let stolen = table.steal().expect("should steal");
        // mid = 400 + (999-400)/2 = 699; tail = 700..=999.
        assert_eq!(stolen.start, 700);
        assert_eq!(stolen.end(), 999);
        assert_eq!(victim.end(), 699);
    }

    #[test]
    fn resume_queues_only_pending() {
        let segs = vec![
            SegmentState { index: 0, start: 0, end: 99, downloaded: 100 }, // complete
            SegmentState { index: 1, start: 100, end: 199, downloaded: 40 }, // pending
        ];
        let table = SegmentTable::from_state(&segs, 10, 16);
        assert_eq!(table.len(), 2);
        assert_eq!(table.queued(), 1);
        let w = table.next_work(true).unwrap();
        assert_eq!(w.index, 1);
        assert_eq!(w.current(), 140);
    }
}
