use hft_solana::update_queue::{DirtyPoolQueue, MarkDirtyResult};

#[test]
fn repeated_updates_collapse_to_one_pending_pool() {
    let queue = DirtyPoolQueue::new(8, 8);

    assert_eq!(queue.mark_dirty(3), MarkDirtyResult::Queued);
    for _ in 0..100 {
        assert_eq!(queue.mark_dirty(3), MarkDirtyResult::AlreadyDirty);
    }

    assert_eq!(queue.len(), 1);
    assert_eq!(queue.pop(), Some(3));
    assert!(queue.is_empty());

    assert_eq!(queue.mark_dirty(3), MarkDirtyResult::Queued);
    assert_eq!(queue.pop(), Some(3));
}

#[test]
fn full_queue_resets_dirty_flag_so_update_can_retry() {
    let queue = DirtyPoolQueue::new(4, 1);

    assert_eq!(queue.mark_dirty(0), MarkDirtyResult::Queued);
    assert_eq!(queue.mark_dirty(1), MarkDirtyResult::QueueFull);

    assert_eq!(queue.pop(), Some(0));

    // Pool 1 must not remain stuck dirty after its failed enqueue.
    assert_eq!(queue.mark_dirty(1), MarkDirtyResult::Queued);
    assert_eq!(queue.pop(), Some(1));
}
