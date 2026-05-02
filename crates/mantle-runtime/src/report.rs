use std::path::PathBuf;

use crate::RuntimeProcessId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    pub artifact_path: PathBuf,
    pub trace_path: PathBuf,
    pub entry_process: String,
    pub entry_message: String,
    pub spawned_processes: Vec<SpawnReport>,
    pub delivered_messages: Vec<MessageDelivery>,
    pub processes: Vec<ProcessReport>,
    pub emitted_outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeReport {
    pub entry_process: String,
    pub entry_message: String,
    pub spawned_processes: Vec<SpawnReport>,
    pub delivered_messages: Vec<MessageDelivery>,
    pub processes: Vec<ProcessReport>,
    pub emitted_outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnReport {
    pub pid: RuntimeProcessId,
    pub process: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDelivery {
    pub pid: RuntimeProcessId,
    pub process: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessReport {
    pub pid: RuntimeProcessId,
    pub process: String,
    pub state: String,
    pub status: ProcessStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    Running,
    Stopped,
}
