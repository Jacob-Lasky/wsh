use bytes::Bytes;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::events::Event;
use super::state::{Query, QueryResponse};

pub async fn run(
    mut raw_rx: broadcast::Receiver<Bytes>,
    mut query_rx: mpsc::Receiver<(Query, oneshot::Sender<QueryResponse>)>,
    _event_tx: broadcast::Sender<Event>,
    cols: usize,
    rows: usize,
    scrollback_limit: usize,
) {
    let mut vt = avt::Vt::builder()
        .size(cols, rows)
        .scrollback_limit(scrollback_limit)
        .build();

    let _seq: u64 = 0;
    let epoch: u64 = 0;
    let mut last_alternate_active = false;

    loop {
        tokio::select! {
            result = raw_rx.recv() => {
                match result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        vt.feed_str(&text);

                        // TODO: Emit events based on changes
                        // For now, just track alternate screen changes
                        let alternate_active = vt.cursor().visible; // placeholder
                        if alternate_active != last_alternate_active {
                            last_alternate_active = alternate_active;
                            // Would emit Mode event
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }

            Some((query, response_tx)) = query_rx.recv() => {
                let response = handle_query(&vt, query, epoch);
                let _ = response_tx.send(response);
            }
        }
    }
}

fn handle_query(vt: &avt::Vt, query: Query, epoch: u64) -> QueryResponse {
    use super::format::format_line;
    use super::state::*;

    match query {
        Query::Screen { format } => {
            let styled = matches!(format, Format::Styled);
            let (cols, rows) = vt.size();
            let cursor = vt.cursor();

            let lines: Vec<_> = vt.view().map(|l| format_line(l, styled)).collect();

            QueryResponse::Screen(ScreenResponse {
                epoch,
                lines,
                cursor: Cursor {
                    row: cursor.row,
                    col: cursor.col,
                    visible: cursor.visible,
                },
                cols,
                rows,
                alternate_active: false, // TODO: track this properly
            })
        }

        Query::Scrollback { format, offset, limit } => {
            let styled = matches!(format, Format::Styled);
            let all_lines: Vec<_> = vt.lines().collect();
            let (_, rows) = vt.size();

            // Scrollback is lines() minus the visible view
            let scrollback_count = all_lines.len().saturating_sub(rows);
            let scrollback_lines: Vec<_> = all_lines
                .into_iter()
                .take(scrollback_count)
                .skip(offset)
                .take(limit)
                .map(|l| format_line(l, styled))
                .collect();

            QueryResponse::Scrollback(ScrollbackResponse {
                epoch,
                lines: scrollback_lines,
                total_lines: scrollback_count,
                offset,
            })
        }

        Query::Cursor => {
            let cursor = vt.cursor();
            QueryResponse::Cursor(CursorResponse {
                epoch,
                cursor: Cursor {
                    row: cursor.row,
                    col: cursor.col,
                    visible: cursor.visible,
                },
            })
        }

        Query::Resize { cols: _, rows: _ } => {
            // Note: vt is not mutable here, this needs refactoring
            // For now, return Ok
            QueryResponse::Ok
        }
    }
}
