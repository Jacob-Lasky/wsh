use serde::{Deserialize, Serialize};

use super::state::{FormattedLine, ScreenResponse};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Line {
        seq: u64,
        index: usize,
        line: FormattedLine,
    },
    Cursor {
        seq: u64,
        row: usize,
        col: usize,
        visible: bool,
    },
    Mode {
        seq: u64,
        alternate_active: bool,
    },
    Reset {
        seq: u64,
        reason: ResetReason,
    },
    Sync {
        seq: u64,
        screen: ScreenResponse,
        scrollback_lines: usize,
    },
    Diff {
        seq: u64,
        changed_lines: Vec<usize>,
        screen: ScreenResponse,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetReason {
    ClearScreen,
    ClearScrollback,
    HardReset,
    AlternateScreenEnter,
    AlternateScreenExit,
    Resize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Subscribe {
    pub events: Vec<EventType>,
    #[serde(default = "default_interval")]
    pub interval_ms: u64,
    #[serde(default)]
    pub format: super::state::Format,
}

fn default_interval() -> u64 {
    100
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Lines,
    Chars,
    Cursor,
    Mode,
    Diffs,
}
