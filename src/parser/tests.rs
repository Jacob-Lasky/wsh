// src/parser/tests.rs
use super::*;
use state::Format;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

/// Parser channel capacity used in tests. Matches the production value
/// in session.rs but is defined separately so tests don't depend on it.
const TEST_PARSER_CHANNEL_CAPACITY: usize = 256;

/// Helper: create a bounded parser channel and spawn a parser.
/// Returns (sender, parser) so tests can feed data via `tx.send().await`.
async fn spawn_test_parser(cols: usize, rows: usize, scrollback_limit: usize) -> (mpsc::Sender<bytes::Bytes>, Parser) {
    let (tx, rx) = mpsc::channel(TEST_PARSER_CHANNEL_CAPACITY);
    let parser = Parser::spawn(rx, cols, rows, scrollback_limit);
    (tx, parser)
}

#[tokio::test]
async fn test_parser_spawn() {
    let (_tx, parser) = spawn_test_parser(80, 24, 1000).await;

    // Should be able to query immediately
    let response = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    match response {
        QueryResponse::Screen(screen) => {
            assert_eq!(screen.cols, 80);
            assert_eq!(screen.rows, 24);
        }
        _ => panic!("expected Screen response"),
    }
}

#[tokio::test]
async fn test_parser_query_cursor() {
    let (_tx, parser) = spawn_test_parser(80, 24, 1000).await;

    let response = parser.query(Query::Cursor).await.unwrap();

    match response {
        QueryResponse::Cursor(cursor_resp) => {
            assert_eq!(cursor_resp.cursor.row, 0);
            assert_eq!(cursor_resp.cursor.col, 0);
        }
        _ => panic!("expected Cursor response"),
    }
}

#[tokio::test]
async fn test_parser_processes_input() {
    let (tx, parser) = spawn_test_parser(80, 24, 1000).await;

    // Send some text through the parser channel
    tx.send(bytes::Bytes::from("Hello, World!")).await.unwrap();

    // Give the parser time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let response = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    match response {
        QueryResponse::Screen(screen) => {
            assert!(!screen.lines.is_empty());
            if let Some(state::FormattedLine::Plain(text)) = screen.lines.first() {
                assert!(text.contains("Hello"));
            }
        }
        _ => panic!("expected Screen response"),
    }
}

#[tokio::test]
async fn test_parser_resize() {
    let (_tx, parser) = spawn_test_parser(80, 24, 1000).await;

    // Resize
    parser.resize(120, 40).await.unwrap();

    // Query screen to verify new size
    let response = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    match response {
        QueryResponse::Screen(screen) => {
            assert_eq!(screen.cols, 120);
            assert_eq!(screen.rows, 40);
        }
        _ => panic!("expected Screen response"),
    }
}

#[tokio::test]
async fn test_parser_scrollback() {
    let (tx, parser) = spawn_test_parser(80, 5, 100).await;

    // Send enough lines to create scrollback
    for i in 0..10 {
        tx.send(bytes::Bytes::from(format!("Line {}\r\n", i))).await.unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let response = parser
        .query(Query::Scrollback {
            format: Format::Plain,
            offset: 0,
            limit: 100,
        })
        .await
        .unwrap();

    match response {
        QueryResponse::Scrollback(scrollback) => {
            // Should have some scrollback
            assert!(scrollback.total_lines > 0);
        }
        _ => panic!("expected Scrollback response"),
    }
}

#[tokio::test]
async fn test_scrollback_includes_all_lines() {
    let (tx, parser) = spawn_test_parser(80, 5, 100).await;

    // Send enough lines to create scrollback
    for i in 0..10 {
        tx.send(bytes::Bytes::from(format!("Line {}\r\n", i))).await.unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // First check screen to see total_lines
    let screen_response = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    let (screen_total_lines, screen_first_line_index) = match &screen_response {
        QueryResponse::Screen(screen) => (screen.total_lines, screen.first_line_index),
        _ => panic!("expected Screen response"),
    };

    // Now check scrollback
    let response = parser
        .query(Query::Scrollback {
            format: Format::Plain,
            offset: 0,
            limit: 100,
        })
        .await
        .unwrap();

    match response {
        QueryResponse::Scrollback(scrollback) => {
            // Verify: with 10 lines fed and 5 rows visible, scrollback should contain ALL lines
            // (both history and current screen)
            assert!(screen_total_lines >= 10, "should have at least 10 total lines, got {}", screen_total_lines);
            assert!(screen_first_line_index >= 5, "first_line_index should be >= 5, got {}", screen_first_line_index);
            // Scrollback total_lines should match screen total_lines (all lines in buffer)
            assert_eq!(scrollback.total_lines, screen_total_lines, "scrollback total_lines should equal screen total_lines");
            assert!(!scrollback.lines.is_empty(), "scrollback lines should not be empty");
        }
        _ => panic!("expected Scrollback response"),
    }
}

#[tokio::test]
async fn test_parser_event_stream() {
    let (tx, parser) = spawn_test_parser(80, 24, 1000).await;

    let mut events = parser.subscribe();

    // Send text
    tx.send(bytes::Bytes::from("Test")).await.unwrap();

    // Should receive events
    let event = tokio::time::timeout(
        tokio::time::Duration::from_millis(100),
        events.next(),
    )
    .await;

    assert!(event.is_ok(), "should receive an event");
}

#[tokio::test]
async fn test_line_event_includes_total_lines() {
    let (tx, parser) = spawn_test_parser(80, 24, 1000).await;

    let mut events = parser.subscribe();

    // Send text to trigger a line event
    tx.send(bytes::Bytes::from("Hello")).await.unwrap();

    // Get the line event
    let sub_event = tokio::time::timeout(
        tokio::time::Duration::from_millis(100),
        events.next(),
    )
    .await
    .expect("should receive event")
    .expect("stream should have item");

    match sub_event {
        SubscriptionEvent::Event(Event::Line { total_lines, .. }) => {
            assert!(total_lines >= 24, "total_lines should be at least screen height");
        }
        other => panic!("expected Line event, got {:?}", other),
    }
}

#[tokio::test]
async fn test_scrollback_when_in_alternate_screen() {
    let (tx, parser) = spawn_test_parser(80, 5, 100).await;

    // Send enough lines to create scrollback
    for i in 0..10 {
        tx.send(bytes::Bytes::from(format!("Line {}\r\n", i))).await.unwrap();
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Verify we have scrollback before switching to alternate screen
    let response = parser
        .query(Query::Scrollback {
            format: Format::Plain,
            offset: 0,
            limit: 100,
        })
        .await
        .unwrap();

    let scrollback_before = match &response {
        QueryResponse::Scrollback(s) => s.total_lines,
        _ => panic!("expected Scrollback response"),
    };
    assert!(scrollback_before > 0, "Should have scrollback before alternate screen");

    // Enter alternate screen mode (DECSET 1049 or smcup)
    tx.send(bytes::Bytes::from("\x1b[?1049h")).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Query scrollback while in alternate screen
    let response = parser
        .query(Query::Scrollback {
            format: Format::Plain,
            offset: 0,
            limit: 100,
        })
        .await
        .unwrap();

    let scrollback_in_alternate = match &response {
        QueryResponse::Scrollback(s) => s.total_lines,
        _ => panic!("expected Scrollback response"),
    };

    // In alternate screen mode, scrollback returns the alternate buffer content
    // (just the current screen, since alternate buffer has no history)
    // Alternate screen should have exactly 5 lines (the screen size)
    assert_eq!(scrollback_in_alternate, 5, "Alternate screen should have screen-size lines");

    // Exit alternate screen mode (DECRST 1049 or rmcup)
    tx.send(bytes::Bytes::from("\x1b[?1049l")).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Query scrollback after exiting alternate screen
    let response = parser
        .query(Query::Scrollback {
            format: Format::Plain,
            offset: 0,
            limit: 100,
        })
        .await
        .unwrap();

    let scrollback_after = match &response {
        QueryResponse::Scrollback(s) => s.total_lines,
        _ => panic!("expected Scrollback response"),
    };

    // Scrollback should be preserved after exiting alternate screen
    assert_eq!(scrollback_after, scrollback_before, "Scrollback should be preserved after exiting alternate screen");
}

#[tokio::test]
async fn test_alternate_active_in_screen_response() {
    let (tx, parser) = spawn_test_parser(80, 24, 1000).await;

    // Initially not in alternate screen
    let response = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    match &response {
        QueryResponse::Screen(screen) => {
            assert!(!screen.alternate_active, "should start in primary screen");
        }
        _ => panic!("expected Screen response"),
    }

    // Enter alternate screen mode
    tx.send(bytes::Bytes::from("\x1b[?1049h")).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let response = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    match &response {
        QueryResponse::Screen(screen) => {
            assert!(screen.alternate_active, "should be in alternate screen after DECSET 1049");
        }
        _ => panic!("expected Screen response"),
    }

    // Exit alternate screen mode
    tx.send(bytes::Bytes::from("\x1b[?1049l")).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let response = parser
        .query(Query::Screen {
            format: Format::Plain,
        })
        .await
        .unwrap();

    match &response {
        QueryResponse::Screen(screen) => {
            assert!(!screen.alternate_active, "should be back in primary screen after DECRST 1049");
        }
        _ => panic!("expected Screen response"),
    }
}

#[tokio::test]
async fn test_alternate_screen_emits_mode_event() {
    let (tx, parser) = spawn_test_parser(80, 24, 1000).await;

    let mut events = parser.subscribe();

    // Enter alternate screen
    tx.send(bytes::Bytes::from("\x1b[?1049h")).await.unwrap();

    // Collect events until we find a Mode event
    let mode_event = tokio::time::timeout(tokio::time::Duration::from_millis(200), async {
        loop {
            if let Some(SubscriptionEvent::Event(Event::Mode { alternate_active, .. })) = events.next().await {
                return alternate_active;
            }
        }
    })
    .await
    .expect("should receive Mode event");

    assert!(mode_event, "Mode event should indicate alternate_active = true");

    // Exit alternate screen
    tx.send(bytes::Bytes::from("\x1b[?1049l")).await.unwrap();

    let mode_event = tokio::time::timeout(tokio::time::Duration::from_millis(200), async {
        loop {
            if let Some(SubscriptionEvent::Event(Event::Mode { alternate_active, .. })) = events.next().await {
                return alternate_active;
            }
        }
    })
    .await
    .expect("should receive Mode event on exit");

    assert!(!mode_event, "Mode event should indicate alternate_active = false");
}

#[tokio::test]
async fn test_screen_response_includes_line_indices() {
    let (tx, parser) = spawn_test_parser(80, 5, 100).await;

    // Send enough lines to create scrollback
    for i in 0..10 {
        tx.send(bytes::Bytes::from(format!("Line {}\r\n", i))).await.unwrap();
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let response = parser
        .query(Query::Screen { format: Format::Plain })
        .await
        .unwrap();

    match response {
        QueryResponse::Screen(screen) => {
            // With 10 lines and 5-row screen, first_line_index should be 5
            // (lines 0-4 in scrollback, lines 5-9 visible)
            assert!(screen.first_line_index > 0, "should have scrollback");
            assert_eq!(screen.total_lines, screen.first_line_index + screen.lines.len());
        }
        _ => panic!("expected Screen response"),
    }
}

#[tokio::test]
async fn test_parser_channel_does_not_lose_data() {
    let (tx, parser) = spawn_test_parser(80, 24, 1000).await;

    // Send 200 messages — all should reach the parser without loss
    for i in 0..200 {
        tx.send(bytes::Bytes::from(format!("msg-{i}\r\n"))).await.unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let response = parser
        .query(Query::Screen { format: Format::Plain })
        .await
        .unwrap();

    match response {
        QueryResponse::Screen(screen) => {
            // total_lines should account for all 200 lines (plus the empty
            // line the cursor sits on after the final \r\n)
            assert!(screen.total_lines >= 200, "should have at least 200 total lines, got {}", screen.total_lines);
        }
        _ => panic!("expected Screen response"),
    }
}
