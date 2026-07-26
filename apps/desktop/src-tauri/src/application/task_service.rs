use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard,
    },
    time::Duration,
};

use tauri::async_runtime::{
    channel, spawn, spawn_blocking, Mutex as AsyncMutex, Receiver, Sender,
};

use crate::domain::task::{TaskId, TaskSnapshot, TaskState};

const DEFAULT_QUEUE_CAPACITY: usize = 128;
const DEFAULT_WORKER_COUNT: usize = 2;

type TaskJob = Box<dyn FnOnce(TaskContext) -> Result<(), TaskFailure> + Send + 'static>;
type TaskRecords = Arc<RwLock<BTreeMap<TaskId, TaskRecord>>>;

struct QueuedTask {
    id: TaskId,
    job: TaskJob,
}

struct TaskRecord {
    snapshot: TaskSnapshot,
    cancellation: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFailure {
    public_message: String,
}

impl TaskFailure {
    pub fn new(public_message: impl Into<String>) -> Self {
        Self {
            public_message: public_message.into(),
        }
    }

    fn into_public_message(self) -> String {
        self.public_message
    }
}

impl fmt::Display for TaskFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.public_message)
    }
}

impl Error for TaskFailure {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskManagerError {
    InvalidName,
    InvalidTotal,
    QueueUnavailable,
}

impl fmt::Display for TaskManagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidName => "task name must not be empty",
            Self::InvalidTotal => "task total must be greater than zero",
            Self::QueueUnavailable => "task queue is full or closed",
        };

        formatter.write_str(message)
    }
}

impl Error for TaskManagerError {}

#[derive(Clone)]
pub struct TaskContext {
    id: TaskId,
    tasks: TaskRecords,
    cancellation: Arc<AtomicBool>,
}

impl TaskContext {
    pub fn id(&self) -> &TaskId {
        &self.id
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.load(Ordering::Acquire)
    }

    /// Reports absolute completed work. Values above the declared total are clamped.
    /// Returns false when the task was cancelled or is no longer running.
    pub fn report_progress(&self, completed: u64) -> bool {
        if self.is_cancelled() {
            return false;
        }

        let mut tasks = write_records(&self.tasks);
        let Some(record) = tasks.get_mut(&self.id) else {
            return false;
        };

        if record.snapshot.state != TaskState::Running || record.cancellation.load(Ordering::Acquire)
        {
            return false;
        }

        record.snapshot.progress.update(completed);
        true
    }
}

pub struct TaskManager {
    sender: Mutex<Option<Sender<QueuedTask>>>,
    tasks: TaskRecords,
    next_id: AtomicU64,
}

impl fmt::Debug for TaskManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskManager")
            .field("task_count", &read_records(&self.tasks).len())
            .finish_non_exhaustive()
    }
}

impl TaskManager {
    pub fn new(queue_capacity: usize, worker_count: usize) -> Self {
        assert!(queue_capacity > 0, "task queue capacity must be positive");
        assert!(worker_count > 0, "task worker count must be positive");

        let (sender, receiver) = channel(queue_capacity);
        let receiver = Arc::new(AsyncMutex::new(receiver));
        let tasks = Arc::new(RwLock::new(BTreeMap::new()));

        for worker_index in 0..worker_count {
            let worker_receiver = Arc::clone(&receiver);
            let worker_tasks = Arc::clone(&tasks);
            drop(spawn(async move {
                worker_loop(worker_index, worker_receiver, worker_tasks).await;
            }));
        }

        Self {
            sender: Mutex::new(Some(sender)),
            tasks,
            next_id: AtomicU64::new(1),
        }
    }

    pub fn submit<F>(
        &self,
        name: impl Into<String>,
        total: u64,
        job: F,
    ) -> Result<TaskSnapshot, TaskManagerError>
    where
        F: FnOnce(TaskContext) -> Result<(), TaskFailure> + Send + 'static,
    {
        let name = name.into().trim().to_owned();
        if name.is_empty() {
            return Err(TaskManagerError::InvalidName);
        }
        if total == 0 {
            return Err(TaskManagerError::InvalidTotal);
        }

        let sequence = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = TaskId::new(format!("task-{sequence:016}"));
        let snapshot = TaskSnapshot::pending(id.clone(), name, total);
        let cancellation = Arc::new(AtomicBool::new(false));
        let queued = QueuedTask {
            id: id.clone(),
            job: Box::new(job),
        };

        let sender = lock_mutex(&self.sender);
        let Some(sender) = sender.as_ref() else {
            return Err(TaskManagerError::QueueUnavailable);
        };

        write_records(&self.tasks).insert(
            id.clone(),
            TaskRecord {
                snapshot: snapshot.clone(),
                cancellation,
            },
        );

        if sender.try_send(queued).is_err() {
            write_records(&self.tasks).remove(&id);
            return Err(TaskManagerError::QueueUnavailable);
        }
        drop(sender);

        Ok(self.get(&id).unwrap_or(snapshot))
    }

    pub fn get(&self, id: &TaskId) -> Option<TaskSnapshot> {
        read_records(&self.tasks)
            .get(id)
            .map(|record| record.snapshot.clone())
    }

    pub fn list(&self) -> Vec<TaskSnapshot> {
        read_records(&self.tasks)
            .values()
            .map(|record| record.snapshot.clone())
            .collect()
    }

    pub fn cancel(&self, id: &TaskId) -> Option<TaskSnapshot> {
        let mut tasks = write_records(&self.tasks);
        let record = tasks.get_mut(id)?;

        if !record.snapshot.state.is_terminal() {
            record.cancellation.store(true, Ordering::Release);
            record.snapshot.state = TaskState::Cancelled;
            record.snapshot.error = None;
        }

        Some(record.snapshot.clone())
    }

    /// Stops accepting work and cooperatively cancels every non-terminal task.
    /// Returns the number of tasks moved to the cancelled state.
    pub fn shutdown(&self) -> usize {
        let sender = lock_mutex(&self.sender).take();
        drop(sender);

        let mut cancelled = 0;
        let mut tasks = write_records(&self.tasks);
        for record in tasks.values_mut() {
            if record.snapshot.state.is_terminal() {
                continue;
            }

            record.cancellation.store(true, Ordering::Release);
            record.snapshot.state = TaskState::Cancelled;
            record.snapshot.error = None;
            cancelled += 1;
        }

        cancelled
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new(DEFAULT_QUEUE_CAPACITY, DEFAULT_WORKER_COUNT)
    }
}

impl Drop for TaskManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub struct TaskService;

impl TaskService {
    pub fn create_progress_task(
        manager: &TaskManager,
        name: String,
        total_steps: u64,
        step_delay: Duration,
    ) -> Result<TaskSnapshot, TaskManagerError> {
        manager.submit(name, total_steps, move |context| {
            for completed in 1..=total_steps {
                if context.is_cancelled() {
                    return Ok(());
                }

                if !step_delay.is_zero() {
                    std::thread::sleep(step_delay);
                }

                if context.is_cancelled() || !context.report_progress(completed) {
                    return Ok(());
                }
            }

            Ok(())
        })
    }

    pub fn get(manager: &TaskManager, id: &TaskId) -> Option<TaskSnapshot> {
        manager.get(id)
    }

    pub fn list(manager: &TaskManager) -> Vec<TaskSnapshot> {
        manager.list()
    }

    pub fn cancel(manager: &TaskManager, id: &TaskId) -> Option<TaskSnapshot> {
        manager.cancel(id)
    }
}

async fn worker_loop(
    worker_index: usize,
    receiver: Arc<AsyncMutex<Receiver<QueuedTask>>>,
    tasks: TaskRecords,
) {
    loop {
        let queued = {
            let mut receiver = receiver.lock().await;
            receiver.recv().await
        };

        let Some(queued) = queued else {
            tracing::debug!(worker_index, "task worker stopped");
            return;
        };

        execute_task(worker_index, queued, &tasks).await;
    }
}

async fn execute_task(worker_index: usize, queued: QueuedTask, tasks: &TaskRecords) {
    let QueuedTask { id, job } = queued;
    let Some(context) = start_task(tasks, &id) else {
        return;
    };

    tracing::debug!(worker_index, task_id = %id, "task started");
    let job_context = context.clone();
    let result = spawn_blocking(move || job(job_context)).await;

    match result {
        Ok(Ok(())) => finish_success(tasks, &id),
        Ok(Err(failure)) => {
            tracing::warn!(worker_index, task_id = %id, error = %failure, "task failed");
            finish_failure(tasks, &id, failure.into_public_message());
        }
        Err(error) => {
            tracing::error!(worker_index, task_id = %id, error = %error, "task worker crashed");
            finish_failure(tasks, &id, "任务执行失败，请重试。".to_owned());
        }
    }
}

fn start_task(tasks: &TaskRecords, id: &TaskId) -> Option<TaskContext> {
    let mut records = write_records(tasks);
    let record = records.get_mut(id)?;

    if record.cancellation.load(Ordering::Acquire) || record.snapshot.state.is_terminal() {
        if !record.snapshot.state.is_terminal() {
            record.snapshot.state = TaskState::Cancelled;
        }
        return None;
    }

    record.snapshot.state = TaskState::Running;
    record.snapshot.error = None;

    Some(TaskContext {
        id: id.clone(),
        tasks: Arc::clone(tasks),
        cancellation: Arc::clone(&record.cancellation),
    })
}

fn finish_success(tasks: &TaskRecords, id: &TaskId) {
    let mut records = write_records(tasks);
    let Some(record) = records.get_mut(id) else {
        return;
    };

    if record.cancellation.load(Ordering::Acquire)
        || record.snapshot.state == TaskState::Cancelled
    {
        record.snapshot.state = TaskState::Cancelled;
        record.snapshot.error = None;
        return;
    }

    if record.snapshot.state.is_terminal() {
        return;
    }

    record.snapshot.progress.complete();
    record.snapshot.state = TaskState::Success;
    record.snapshot.error = None;
}

fn finish_failure(tasks: &TaskRecords, id: &TaskId, public_message: String) {
    let mut records = write_records(tasks);
    let Some(record) = records.get_mut(id) else {
        return;
    };

    if record.cancellation.load(Ordering::Acquire)
        || record.snapshot.state == TaskState::Cancelled
    {
        record.snapshot.state = TaskState::Cancelled;
        record.snapshot.error = None;
        return;
    }

    if record.snapshot.state.is_terminal() {
        return;
    }

    record.snapshot.state = TaskState::Failed;
    record.snapshot.error = Some(public_message);
}

fn lock_mutex<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn read_records(records: &RwLock<BTreeMap<TaskId, TaskRecord>>) -> RwLockReadGuard<'_, BTreeMap<TaskId, TaskRecord>> {
    records
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_records(
    records: &RwLock<BTreeMap<TaskId, TaskRecord>>,
) -> RwLockWriteGuard<'_, BTreeMap<TaskId, TaskRecord>> {
    records
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread, time::Instant};

    use super::*;

    #[test]
    fn executes_queued_tasks_in_fifo_order_with_one_worker() {
        let manager = TaskManager::new(8, 1);
        let order = Arc::new(Mutex::new(Vec::new()));

        let first_order = Arc::clone(&order);
        let first = manager
            .submit("first", 1, move |context| {
                thread::sleep(Duration::from_millis(10));
                lock_mutex(&first_order).push(1);
                context.report_progress(1);
                Ok(())
            })
            .expect("first task should be queued");

        let second_order = Arc::clone(&order);
        let second = manager
            .submit("second", 1, move |context| {
                lock_mutex(&second_order).push(2);
                context.report_progress(1);
                Ok(())
            })
            .expect("second task should be queued");

        assert_eq!(wait_for_terminal(&manager, &first.id).state, TaskState::Success);
        assert_eq!(wait_for_terminal(&manager, &second.id).state, TaskState::Success);
        assert_eq!(*lock_mutex(&order), vec![1, 2]);
    }

    #[test]
    fn tracks_progress_and_completes_successfully() {
        let manager = TaskManager::new(4, 1);
        let created = manager
            .submit("index files", 4, |context| {
                for completed in 1..=4 {
                    assert!(context.report_progress(completed));
                }
                Ok(())
            })
            .expect("task should be queued");

        let completed = wait_for_terminal(&manager, &created.id);

        assert_eq!(completed.state, TaskState::Success);
        assert_eq!(completed.progress.completed, 4);
        assert_eq!(completed.progress.percentage, 100);
    }

    #[test]
    fn cooperatively_cancels_running_work() {
        let manager = TaskManager::new(4, 1);
        let created = manager
            .submit("cancel me", 500, |context| {
                for completed in 1..=500 {
                    if context.is_cancelled() {
                        return Ok(());
                    }
                    thread::sleep(Duration::from_millis(2));
                    if !context.report_progress(completed) {
                        return Ok(());
                    }
                }
                Ok(())
            })
            .expect("task should be queued");

        wait_for_state(&manager, &created.id, TaskState::Running);
        let cancelled = manager
            .cancel(&created.id)
            .expect("running task should exist");

        assert_eq!(cancelled.state, TaskState::Cancelled);
        assert_eq!(
            wait_for_terminal(&manager, &created.id).state,
            TaskState::Cancelled
        );
    }

    #[test]
    fn captures_safe_task_failure_message() {
        let manager = TaskManager::new(4, 1);
        let created = manager
            .submit("fail", 1, |_context| {
                Err(TaskFailure::new("任务处理失败。"))
            })
            .expect("task should be queued");

        let failed = wait_for_terminal(&manager, &created.id);

        assert_eq!(failed.state, TaskState::Failed);
        assert_eq!(failed.error.as_deref(), Some("任务处理失败。"));
    }

    #[test]
    fn shutdown_cancels_tasks_and_rejects_new_work() {
        let manager = TaskManager::new(4, 1);
        let created = manager
            .submit("long running", 100, |context| {
                while !context.is_cancelled() {
                    thread::sleep(Duration::from_millis(2));
                }
                Ok(())
            })
            .expect("task should be queued");

        wait_for_state(&manager, &created.id, TaskState::Running);
        assert_eq!(manager.shutdown(), 1);
        assert_eq!(
            manager.get(&created.id).expect("task should exist").state,
            TaskState::Cancelled
        );
        assert_eq!(
            manager.submit("rejected", 1, |_context| Ok(())),
            Err(TaskManagerError::QueueUnavailable)
        );
    }

    fn wait_for_state(manager: &TaskManager, id: &TaskId, expected: TaskState) -> TaskSnapshot {
        wait_for(manager, id, |snapshot| snapshot.state == expected)
    }

    fn wait_for_terminal(manager: &TaskManager, id: &TaskId) -> TaskSnapshot {
        wait_for(manager, id, |snapshot| snapshot.state.is_terminal())
    }

    fn wait_for(
        manager: &TaskManager,
        id: &TaskId,
        predicate: impl Fn(&TaskSnapshot) -> bool,
    ) -> TaskSnapshot {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = manager.get(id).expect("task should exist");
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "task state transition timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }
}
