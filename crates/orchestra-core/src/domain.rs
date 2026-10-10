use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Blocked,
    Interrupted,
}

impl Status {
    pub fn terminal(self) -> bool {
        !matches!(self, Self::Queued | Self::Running)
    }
    pub fn allows(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Queued,
                Self::Running | Self::Cancelled | Self::Blocked
            ) | (
                Self::Running,
                Self::Succeeded
                    | Self::Failed
                    | Self::Cancelled
                    | Self::TimedOut
                    | Self::Interrupted
            )
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewTask {
    pub title: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default = "demo")]
    pub provider: String,
    #[serde(default = "duration")]
    pub estimate_ms: u64,
    #[serde(default = "duration")]
    pub demo_duration_ms: u64,
    #[serde(default = "timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub demo_fail: bool,
}
fn demo() -> String {
    "demo".into()
}
fn duration() -> u64 {
    1000
}
fn timeout() -> u64 {
    30000
}

impl NewTask {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() || self.title.len() > 200 || self.prompt.len() > 16000 {
            return Err("title must be 1..200 bytes; prompt at most 16000 bytes".into());
        }
        if !(1..=3_600_000).contains(&self.estimate_ms)
            || !(1..=3_600_000).contains(&self.demo_duration_ms)
            || !(10..=3_600_000).contains(&self.timeout_ms)
            || self.priority > 10
        {
            return Err(
                "invalid duration (1..3600000), timeout (10..3600000), or priority (0..10)".into(),
            );
        }
        if self.depends_on.len() > 32 {
            return Err("at most 32 dependencies".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    #[serde(flatten)]
    pub spec: NewTask,
    pub status: Status,
    pub created_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub cancel_requested: bool,
    pub output: String,
    pub detail: String,
    pub revision: u64,
}

impl Task {
    pub fn new(spec: NewTask) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            spec,
            status: Status::Queued,
            created_at_ms: now_ms(),
            started_at_ms: None,
            finished_at_ms: None,
            cancel_requested: false,
            output: String::new(),
            detail: "waiting for a worker".into(),
            revision: 0,
        }
    }
    pub fn transition(&mut self, next: Status, detail: impl Into<String>) -> Result<(), String> {
        if !self.status.allows(next) {
            return Err(format!("invalid transition {:?} → {:?}", self.status, next));
        }
        self.status = next;
        self.detail = detail.into();
        self.revision += 1;
        if next == Status::Running {
            self.started_at_ms = Some(now_ms());
        }
        if next.terminal() {
            self.finished_at_ms = Some(now_ms());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_states_cannot_restart() {
        for status in [
            Status::Succeeded,
            Status::Failed,
            Status::Cancelled,
            Status::TimedOut,
            Status::Interrupted,
            Status::Blocked,
        ] {
            assert!(!status.allows(Status::Running));
        }
        assert!(Status::Queued.allows(Status::Cancelled));
        assert!(Status::Running.allows(Status::TimedOut));
        assert!(!Status::Queued.allows(Status::Succeeded));
    }
    #[test]
    fn reject_unbounded_input() {
        let mut spec: NewTask = serde_json::from_str(r#"{"title":"test"}"#).unwrap();
        assert!(spec.validate().is_ok());
        spec.estimate_ms = 0;
        assert!(spec.validate().is_err());
    }
}
