//! Types related to task management

use crate::syscall::MAX_SYSCALL_ID;

use super::TaskContext;

/// The task control block (TCB) of a task.
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// The syscall tracer
    pub syscall_tracer: SyscallTracer,
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
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

const TIMES_SIZE: usize = MAX_SYSCALL_ID + 1;

/// The syscall tracer
#[derive(Copy, Clone)]
pub struct SyscallTracer {
    pub counts: [usize; TIMES_SIZE],
}

impl SyscallTracer {
    pub fn new() -> Self {
        Self {
            counts: [0; TIMES_SIZE],
        }
    }

    pub fn increase_count(&mut self, syscall_id: usize) {
        assert!(syscall_id < TIMES_SIZE, "syscall_id out of range");
        self.counts[syscall_id] += 1;
    }

    pub fn get_count(&self, syscall_id: usize) -> isize {
        assert!(syscall_id < TIMES_SIZE, "syscall_id out of range");
        self.counts[syscall_id] as isize
    }
}
