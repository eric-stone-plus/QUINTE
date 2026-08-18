//! Task records: in-memory state, JSONL-persisted so a restart does not
//! silently orphan a task the orchestrator still polls.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Working,
    Completed,
    Failed,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TaskRecord {
    pub task_id: String,
    pub state: TaskState,
    pub artifact: Option<Value>,
    pub error: Option<String>,
    pub created_at: String,
}

impl TaskRecord {
    fn to_json(&self) -> Value {
        json!({
            "task_id": self.task_id,
            "state": self.state.as_str(),
            "artifact": self.artifact,
            "error": self.error,
            "created_at": self.created_at,
        })
    }

    fn from_json(value: Value) -> Option<Self> {
        let state = match value.get("state")?.as_str()? {
            "working" => TaskState::Working,
            "completed" => TaskState::Completed,
            "failed" => TaskState::Failed,
            _ => return None,
        };
        Some(Self {
            task_id: value.get("task_id")?.as_str()?.to_string(),
            state,
            artifact: value.get("artifact").cloned(),
            error: value.get("error").and_then(Value::as_str).map(str::to_string),
            created_at: value
                .get("created_at")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }
}

pub struct TaskStore {
    path: PathBuf,
    tasks: Vec<TaskRecord>,
}

impl TaskStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut tasks = Vec::new();
        if path.exists() {
            let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
            for line in BufReader::new(file).lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let value: Value = serde_json::from_str(&line)
                    .with_context(|| format!("corrupt task record in {}", path.display()))?;
                if let Some(record) = TaskRecord::from_json(value) {
                    tasks.push(record);
                }
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            tasks,
        })
    }

    pub fn get(&self, task_id: &str) -> Option<&TaskRecord> {
        self.tasks.iter().find(|t| t.task_id == task_id)
    }

    pub fn insert(&mut self, record: TaskRecord) -> Result<()> {
        self.persist(&record)?;
        self.tasks.push(record);
        Ok(())
    }

    pub fn update(&mut self, record: &TaskRecord) -> Result<()> {
        self.persist(record)?;
        if let Some(slot) = self.tasks.iter_mut().find(|t| t.task_id == record.task_id) {
            *slot = record.clone();
        }
        Ok(())
    }

    fn persist(&self, record: &TaskRecord) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", record.to_json())?;
        file.sync_all()?;
        Ok(())
    }
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_roundtrips_and_survives_reopen() {
        let dir = std::env::temp_dir().join(format!("pi-store-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tasks.jsonl");
        {
            let mut store = TaskStore::open(&path).unwrap();
            store
                .insert(TaskRecord {
                    task_id: "t-1".into(),
                    state: TaskState::Working,
                    artifact: None,
                    error: None,
                    created_at: now(),
                })
                .unwrap();
        }
        let store = TaskStore::open(&path).unwrap();
        assert_eq!(store.get("t-1").unwrap().state, TaskState::Working);
        std::fs::remove_dir_all(&dir).ok();
    }
}
