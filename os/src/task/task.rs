//! Types related to task management

use super::TaskContext;
use crate::config::{MAX_SYSCALL_NUM};

/// The task control block (TCB) of a task.
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// The fist time task start
    pub task_start_time: usize,
    /// The syscall times
    pub syscall_times: [u32; MAX_SYSCALL_NUM]
}

/// The status of a task
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}
