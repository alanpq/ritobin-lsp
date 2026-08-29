//! Readiness of the background resources the server needs before its answers can be trusted.
//!
//! Completion, hover and the linter are all only as good as the meta dump and the hashtables. Until
//! those have loaded the server still answers, it just answers with hex names and no classes, which
//! is indistinguishable from "broken" unless we say so.

use crate::lsp::ext::{Health, ServerStatusParams};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TaskStatus {
    /// Queued, not started yet.
    #[default]
    Pending,
    /// Running, with something to show the user.
    Loading(String),
    /// Done and usable.
    Ready,
    /// Gave up. The server still runs, just degraded.
    Failed(String),
}

impl TaskStatus {
    fn settled(&self) -> bool {
        matches!(self, Self::Ready | Self::Failed(_))
    }
}

/// Aggregate readiness, reported to the client as `experimental/serverStatus`.
#[derive(Debug, Default)]
pub struct ServerStatus {
    pub hashes: TaskStatus,
    pub meta: TaskStatus,
}

impl ServerStatus {
    /// Ordered so the message reads meta-first, which is what users wait on longest.
    fn tasks(&self) -> [&TaskStatus; 2] {
        [&self.meta, &self.hashes]
    }

    pub fn params(&self) -> ServerStatusParams {
        let tasks = self.tasks();

        // Work in progress is what the user wants to see; a past failure only surfaces once
        // nothing is still running.
        let loading: Vec<&str> = tasks
            .iter()
            .filter_map(|task| match task {
                TaskStatus::Loading(msg) => Some(msg.as_str()),
                _ => None,
            })
            .collect();

        let message = match loading.is_empty() {
            false => Some(loading.join(", ")),
            true => {
                let failures: Vec<&str> = tasks
                    .iter()
                    .filter_map(|task| match task {
                        TaskStatus::Failed(msg) => Some(msg.as_str()),
                        _ => None,
                    })
                    .collect();
                (!failures.is_empty()).then(|| failures.join(", "))
            }
        };

        ServerStatusParams {
            health: match tasks.iter().any(|t| matches!(t, TaskStatus::Failed(_))) {
                true => Health::Warning,
                false => Health::Ok,
            },
            quiescent: tasks.iter().all(|task| task.settled()),
            message,
        }
    }
}

#[cfg(test)]
mod tests;
