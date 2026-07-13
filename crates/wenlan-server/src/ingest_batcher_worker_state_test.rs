use super::IngestBatcher;

#[test]
fn observes_open_and_closed_sender_without_submitting_work() {
    let (open_tx, open_rx) = tokio::sync::mpsc::channel(1);
    let open = IngestBatcher { tx: open_tx };
    assert!(!open.is_closed());
    assert_eq!(open_rx.len(), 0);

    let (closed_tx, closed_rx) = tokio::sync::mpsc::channel(1);
    let closed = IngestBatcher { tx: closed_tx };
    drop(closed_rx);
    assert!(closed.is_closed());
}
