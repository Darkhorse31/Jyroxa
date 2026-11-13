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

/// Do not start a probe when less than this remains in the download.
pub const PROBE_MIN_REMAINING: u64 = 8 << 20;
/// Bytes the primary worker should read before a probe may cap its range.
pub const PROBE_LEAD: u64 = 4 << 20;

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
    /// Cursor confirmed by the disk writer, used for crash-safe checkpoints.
    pub(crate) committed: AtomicU64,
    /// Serializes a split with accepting a network chunk. Never held across I/O.
    pub(crate) bounds: Mutex<()>,
    /// Set when the segment is fully downloaded.
    pub done: AtomicBool,
    /// Inclusive byte cap for the primary worker (probe splits); 0 = none.
    cap_end: AtomicU64,
    /// True while a worker is actively downloading this segment.
    in_use: AtomicBool,
}

impl Segment {
    pub fn new(index: usize, start: u64, end: u64, current: u64) -> Arc<Self> {
        Arc::new(Segment {
            index,
            start,
            end: AtomicU64::new(end),
            current: AtomicU64::new(current),
            committed: AtomicU64::new(current),
            bounds: Mutex::new(()),
            done: AtomicBool::new(current > end),
            cap_end: AtomicU64::new(0),
            in_use: AtomicBool::new(false),
        })
    }

    pub fn end(&self) -> u64 {
        self.end.load(Ordering::Acquire)
    }

    /// Last inclusive byte this worker may read (probe cap or full segment end).
    pub fn download_end(&self) -> u64 {
        let end = self.end();
        let cap = self.cap_end.load(Ordering::Acquire);
        if cap > 0 && cap < end {
            cap
        } else {
            end
        }
    }

    pub fn set_in_use(&self, active: bool) {
        self.in_use.store(active, Ordering::Release);
    }

    pub fn is_in_use(&self) -> bool {
        self.in_use.load(Ordering::Acquire)
    }

    /// Stop the primary stream at `cap` without shrinking `end` until the cap is reached.
    pub fn set_cap_end(&self, cap: u64) {
        self.cap_end.store(cap, Ordering::Release);
    }

    pub fn clear_cap_end(&self) {
        self.cap_end.store(0, Ordering::Release);
    }

    /// Commit a probe cap into the permanent inclusive end.
    pub fn apply_cap_if_due(&self, pos: u64) {
        let cap = self.cap_end.load(Ordering::Acquire);
        if cap == 0 {
            return;
        }
        if pos > cap {
            self.end.store(cap, Ordering::Release);
            self.cap_end.store(0, Ordering::Release);
        }
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
    /// Cap on *unfinished* segments, to avoid handshake storms on lossy links.
    /// Finished segments don't count, so idle workers can keep stealing work
    /// until the end of the file.
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
        !segs.is_empty() && segs.iter().all(|s| s.is_done())
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

    /// A checkpoint must capture the range topology and disk cursors together;
    /// otherwise a concurrent split could omit the newly created tail.
    pub(crate) fn committed_snapshot(&self) -> Vec<SegmentState> {
        self.all
            .lock()
            .unwrap()
            .iter()
            .map(|seg| {
                let mut snapshot = seg.snapshot();
                snapshot.downloaded = seg
                    .committed
                    .load(Ordering::Acquire)
                    .min(snapshot.end.saturating_add(1))
                    .saturating_sub(snapshot.start);
                snapshot
            })
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
        while let Some(s) = self.queue.lock().unwrap().pop_front() {
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

    /// Queue half of the largest remaining range for an extra connection.
    pub fn try_queue_probe_split(&self, total_remaining: u64) -> bool {
        if total_remaining < PROBE_MIN_REMAINING {
            return false;
        }
        let min_tail = self.min_steal.saturating_mul(2);
        let mut segs = self.all.lock().unwrap();
        if live_segments(&segs) >= self.steal_cap {
            return false;
        }

        let mut best: Option<(usize, u64, u64)> = None;
        for (i, s) in segs.iter().enumerate() {
            if s.is_done() {
                continue;
            }
            let cur = s.current();
            let end = s.end();
            let rem = end.saturating_sub(cur).saturating_add(1);
            if rem < min_tail {
                continue;
            }
            let replace = best.map(|(_, r, _)| rem > r).unwrap_or(true);
            if replace {
                best = Some((i, rem, cur));
            }
        }
        let (victim_idx, _, _) = match best {
            Some(v) => v,
            None => return false,
        };
        let victim = segs[victim_idx].clone();
        let Some(stolen) = split_segment_tail(&mut segs, &victim, min_tail, &self.next_index)
        else {
            return false;
        };
        self.queue.lock().unwrap().push_back(stolen);
        true
    }

    /// Split the tail of a specific in-progress segment after a stall.
    pub fn split_tail(&self, victim: &Arc<Segment>) -> Option<Arc<Segment>> {
        let mut segs = self.all.lock().unwrap();
        if live_segments(&segs) >= self.steal_cap {
            return None;
        }
        split_segment_tail(
            &mut segs,
            victim,
            self.min_steal.max(64 * 1024),
            &self.next_index,
        )
    }

    fn try_steal(&self, min_tail: u64) -> Option<Arc<Segment>> {
        let mut segs = self.all.lock().unwrap();
        if live_segments(&segs) >= self.steal_cap {
            return None;
        }

        let mut best: Option<(usize, u64, u64)> = None;
        for (i, s) in segs.iter().enumerate() {
            if s.is_done() {
                continue;
            }
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

fn live_segments(segs: &[Arc<Segment>]) -> usize {
    segs.iter().filter(|s| s.remaining() > 0).count()
}

fn split_segment_tail(
    segs: &mut Vec<Arc<Segment>>,
    victim: &Arc<Segment>,
    min_tail: u64,
    next_index: &AtomicU64,
) -> Option<Arc<Segment>> {
    let _bounds = victim.bounds.lock().unwrap();
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
    fn finished_segments_do_not_block_stealing() {
        // Cap of 8 live segments; finish segments as we go and keep stealing.
        let table = SegmentTable::from_ranges(&[(0, 1u64 << 40)], 10, 8);
        let mut seg = table.next_work(true).unwrap();
        for _ in 0..20 {
            let stolen = table.next_work(true).expect("must still be able to steal");
            // The victim finishes its (now shorter) range.
            seg.current.store(seg.end() + 1, Ordering::Release);
            seg.mark_done();
            seg = stolen;
        }
        assert!(table.len() > 8, "total segments may exceed the live cap");
    }

    #[test]
    fn live_cap_still_limits_concurrency() {
        let table = SegmentTable::from_ranges(&[(0, 1_000_000)], 10, 8);
        let mut held = vec![table.next_work(true).unwrap()];
        while let Some(s) = table.next_work(true) {
            held.push(s);
            assert!(held.len() <= 8);
        }
        assert_eq!(held.len(), 8);
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
    fn probe_split_shortens_an_active_segment_without_overlap() {
        let table = SegmentTable::from_ranges(&[(0, 16_777_215)], PROBE_LEAD, 16);
        let victim = table.next_work(false).unwrap();
        victim.set_in_use(true);
        victim.current.store(1024 * 1024, Ordering::Release);
        assert!(table.try_queue_probe_split(16 << 20));
        assert_eq!(table.queued(), 1);
        let tail = table.queue.lock().unwrap().pop_front().unwrap();
        assert_eq!(tail.start, victim.end() + 1);
        assert_eq!(tail.end(), 16_777_215);
        assert!(victim.end() >= victim.current());
    }

    #[test]
    fn idle_worker_splits_largest_active_range() {
        let table = SegmentTable::from_ranges(&[(0, 999), (1000, 2999)], 10, 16);
        let first = table.next_work(false).unwrap();
        let largest = table.next_work(false).unwrap();
        first.set_in_use(true);
        largest.set_in_use(true);
        largest.current.store(1400, Ordering::Release);
        let tail = table
            .next_work(true)
            .expect("active range must be stealable");
        assert_eq!(first.end(), 999);
        assert_eq!(largest.end(), 2199);
        assert_eq!(tail.start, 2200);
        assert_eq!(tail.end(), 2999);
    }

    #[test]
    fn network_completion_waits_for_final_write_submission() {
        let table = SegmentTable::from_ranges(&[(0, 999)], 10, 8);
        let seg = table.next_work(false).unwrap();
        seg.current.store(1000, Ordering::Release);
        assert!(!table.all_done());
        seg.mark_done();
        assert!(table.all_done());
    }

    #[test]
    fn resume_queues_only_pending() {
        let segs = vec![
            SegmentState {
                index: 0,
                start: 0,
                end: 99,
                downloaded: 100,
            }, // complete
            SegmentState {
                index: 1,
                start: 100,
                end: 199,
                downloaded: 40,
            }, // pending
        ];
        let table = SegmentTable::from_state(&segs, 10, 16);
        assert_eq!(table.len(), 2);
        assert_eq!(table.queued(), 1);
        let w = table.next_work(true).unwrap();
        assert_eq!(w.index, 1);
        assert_eq!(w.current(), 140);
    }
}
