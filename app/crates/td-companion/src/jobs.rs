//! Jobs: slow Device work that answers `202` at once and reports through `job.updated`.
//! One Job holds a Device or New board at a time. Companion-scoped Jobs share a limit per kind.
use crate::api::{ApiError, Clock, CommonError, ErrorBody, Event, Events};
use axum::http::StatusCode;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, VecDeque},
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use ts_rs::TS;

/// Finished Jobs `GET /jobs` keeps, and for how long.
const KEEP_FINISHED: usize = 50;
const KEEP_FOR: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "id", rename_all = "kebab-case")]
pub enum JobScope {
    Device(String),
    NewBoard(String),
    Companion,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    WaitingForConfirmation,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
impl JobState {
    pub fn finished(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub scope: JobScope,
    pub state: JobState,
    /// Typed per kind in `JobMap`.
    pub progress: serde_json::Value,
    /// Typed per kind in `JobMap`. Set once the Job succeeds.
    pub result: Option<serde_json::Value>,
    pub error: Option<ErrorBody>,
    /// Whether the current phase allows `DELETE /jobs/{id}`.
    pub cancellable: bool,
    pub created_ms: u64,
    pub finished_ms: Option<u64>,
    pub rev: u64,
}

/// A kind of Job, registered by its feature with `Feature::job`.
pub trait JobKind: 'static {
    const NAME: &'static str;
    /// How many Companion-scoped Jobs of this kind may run at once. Device-scoped Jobs are
    /// limited by their Device instead.
    const COMPANION_LIMIT: usize = 1;
    type Progress: Serialize + TS + 'static;
    type Result: Serialize + DeserializeOwned + TS + 'static;
}

#[derive(Serialize, Deserialize, TS)]
pub struct JobUpdated(pub Job);
impl Event for JobUpdated {
    const NAME: &'static str = "job.updated";
    fn device(&self) -> Option<&str> {
        match &self.0.scope {
            JobScope::Device(id) | JobScope::NewBoard(id) => Some(id),
            JobScope::Companion => None,
        }
    }
}

/// Why a Job's work ended without a result.
#[derive(Debug)]
pub enum Stop {
    Cancelled,
    Failed(ErrorBody),
}
impl<E: ApiError> From<E> for Stop {
    fn from(error: E) -> Self {
        Self::Failed(error.body())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("A Job already holds the Device.")]
    Busy(Box<Job>),
    #[error("The Job does not exist.")]
    NotFound,
    #[error("The Job can't be cancelled in its current phase.")]
    NotCancellable,
    /// The error a cancelled Job carries.
    #[error("The Job was cancelled.")]
    Cancelled,
    #[error(transparent)]
    Common(#[from] CommonError),
}
impl ApiError for JobError {
    const CODES: &'static [&'static str] = &[
        "device.busy",
        "job.not_found",
        "job.not_cancellable",
        "job.cancelled",
    ];
    fn status(&self) -> StatusCode {
        match self {
            Self::Busy(_) | Self::NotCancellable | Self::Cancelled => StatusCode::CONFLICT,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Common(e) => e.status(),
        }
    }
    fn code(&self) -> &'static str {
        match self {
            Self::Busy(_) => "device.busy",
            Self::NotFound => "job.not_found",
            Self::NotCancellable => "job.not_cancellable",
            Self::Cancelled => "job.cancelled",
            Self::Common(e) => e.code(),
        }
    }
    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::Busy(job) => Some(serde_json::json!({ "job": job })),
            Self::Common(e) => e.details(),
            _ => None,
        }
    }
}

struct Entry {
    job: Job,
    cancel: Arc<AtomicBool>,
}
struct InstallLease {
    id: String,
    owner: String,
    until: Instant,
}
#[derive(Default)]
struct Table {
    lease: Option<InstallLease>,
    active: BTreeMap<String, Entry>,
    finished: VecDeque<(Instant, Job)>,
}
impl Table {
    fn prune(&mut self, now: Instant) {
        while self.finished.len() > KEEP_FINISHED
            || self
                .finished
                .front()
                .is_some_and(|(at, _)| now.saturating_duration_since(*at) > KEEP_FOR)
        {
            self.finished.pop_front();
        }
    }
}

/// Holds Jobs before they run, so tests can see a Job while it's active.
#[cfg(any(test, feature = "test-hooks"))]
#[derive(Default)]
struct Gate {
    closed: Mutex<bool>,
    open: parking_lot::Condvar,
}

#[derive(Clone)]
pub struct Jobs {
    table: Arc<Mutex<Table>>,
    events: Events,
    clock: Clock,
    session_active: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    #[cfg(any(test, feature = "test-hooks"))]
    gate: Arc<Gate>,
}
impl Jobs {
    pub(crate) fn new(events: Events, clock: Clock, session_active: Arc<dyn Fn(&str) -> bool + Send + Sync>) -> Self {
        Self {
            table: Default::default(),
            events,
            clock,
            session_active,
            #[cfg(any(test, feature = "test-hooks"))]
            gate: Default::default(),
        }
    }
    fn expire_lease(&self, table: &mut Table) {
        if table.lease.as_ref().is_some_and(|l| self.clock.now() >= l.until || !(self.session_active)(&l.owner)) {
            table.lease = None;
        }
    }
    pub fn install_lock(&self, owner: &str) -> Result<String, CommonError> {
        let mut table = self.table.lock();
        self.expire_lease(&mut table);
        if !table.active.is_empty() || table.lease.is_some() { return Err(CommonError::SystemBusy); }
        let id = new_id();
        table.lease = Some(InstallLease { id: id.clone(), owner: owner.into(), until: self.clock.now() + KEEP_FOR });
        Ok(id)
    }
    pub fn install_renew(&self, owner: &str, id: &str, release: bool) -> Result<(), CommonError> {
        let mut table = self.table.lock();
        self.expire_lease(&mut table);
        let lease = table.lease.as_mut().filter(|l| l.owner == owner && l.id == id)
            .ok_or(CommonError::Forbidden("Only the lock owner may change it."))?;
        lease.until = self.clock.now() + KEEP_FOR;
        if release { table.lease = None; }
        Ok(())
    }
    /// Active Jobs, then the finished ones that are kept, oldest first.
    pub fn list(&self) -> Vec<Job> {
        let mut table = self.table.lock();
        table.prune(self.clock.now());
        let mut jobs: Vec<Job> = table.active.values().map(|e| e.job.clone()).collect();
        jobs.sort_by_key(|j| j.created_ms);
        jobs.extend(table.finished.iter().map(|(_, j)| j.clone()));
        jobs
    }
    pub fn get(&self, id: &str) -> Option<Job> {
        let mut table = self.table.lock();
        table.prune(self.clock.now());
        table.active.get(id).map(|e| e.job.clone()).or_else(|| {
            table
                .finished
                .iter()
                .find(|(_, j)| j.id == id)
                .map(|(_, j)| j.clone())
        })
    }
    /// Asks an active Job to stop. Its work ends at its next check, and the Job then reads
    /// `cancelled`.
    pub fn cancel(&self, id: &str) -> Result<Job, JobError> {
        let table = self.table.lock();
        match table.active.get(id) {
            Some(entry) if entry.job.cancellable => {
                entry.cancel.store(true, Ordering::SeqCst);
                Ok(entry.job.clone())
            }
            Some(_) => Err(JobError::NotCancellable),
            None if table.finished.iter().any(|(_, j)| j.id == id) => Err(JobError::NotCancellable),
            None => Err(JobError::NotFound),
        }
    }
    /// The Job that holds a Device or New board, if any.
    pub fn holding(&self, scope: &JobScope) -> Option<Job> {
        if *scope == JobScope::Companion {
            return None;
        }
        let table = self.table.lock();
        table
            .active
            .values()
            .find(|e| e.job.scope == *scope)
            .map(|e| e.job.clone())
    }
    /// Starts `work` on a blocking thread and returns the queued Job. A Device or New board
    /// that a Job already holds gives `device.busy`; a Companion-scoped kind at its limit gives
    /// `limits.exceeded`.
    pub fn start<K: JobKind>(
        &self,
        scope: JobScope,
        cancellable: bool,
        work: impl FnOnce(&JobHandle<K>) -> Result<K::Result, Stop> + Send + 'static,
    ) -> Result<Job, JobError> {
        let cancel = Arc::new(AtomicBool::new(false));
        let job = {
            let mut table = self.table.lock();
            table.prune(self.clock.now());
            self.expire_lease(&mut table);
            if table.lease.is_some() { return Err(CommonError::Installing.into()); }
            if scope == JobScope::Companion {
                let running = table
                    .active
                    .values()
                    .filter(|e| e.job.kind == K::NAME)
                    .count();
                if running >= K::COMPANION_LIMIT {
                    return Err(
                        CommonError::Limits("Too many Jobs of this kind are running.").into(),
                    );
                }
            } else if let Some(e) = table.active.values().find(|e| e.job.scope == scope) {
                return Err(JobError::Busy(Box::new(e.job.clone())));
            }
            let job = Job {
                id: new_id(),
                kind: K::NAME.into(),
                scope,
                state: JobState::Queued,
                progress: serde_json::Value::Null,
                result: None,
                error: None,
                cancellable,
                created_ms: crate::now_ms(),
                finished_ms: None,
                rev: 1,
            };
            table.active.insert(
                job.id.clone(),
                Entry {
                    job: job.clone(),
                    cancel: cancel.clone(),
                },
            );
            job
        };
        self.events.publish(&JobUpdated(job.clone()));
        let handle = JobHandle::<K> {
            jobs: self.clone(),
            id: job.id.clone(),
            cancel,
            kind: PhantomData,
        };
        tokio::task::spawn_blocking(move || {
            handle.update(|j| j.state = JobState::Running);
            #[cfg(any(test, feature = "test-hooks"))]
            handle.jobs.pass_gate();
            let outcome = handle.check().and_then(|()| work(&handle));
            handle.jobs.finish(&handle.id, outcome);
        });
        Ok(job)
    }
    fn update(&self, id: &str, change: impl FnOnce(&mut Job)) {
        let job = {
            let mut table = self.table.lock();
            let Some(entry) = table.active.get_mut(id) else {
                return;
            };
            change(&mut entry.job);
            entry.job.rev += 1;
            entry.job.clone()
        };
        self.events.publish(&JobUpdated(job));
    }
    fn finish<T: Serialize>(&self, id: &str, outcome: Result<T, Stop>) {
        let job = {
            let mut table = self.table.lock();
            let Some(Entry { mut job, .. }) = table.active.remove(id) else {
                return;
            };
            match outcome {
                Ok(result) => {
                    job.state = JobState::Succeeded;
                    job.result = Some(serde_json::to_value(result).unwrap());
                }
                Err(Stop::Cancelled) => {
                    job.state = JobState::Cancelled;
                    job.error = Some(JobError::Cancelled.body());
                }
                Err(Stop::Failed(error)) => {
                    job.state = JobState::Failed;
                    job.error = Some(error);
                }
            }
            job.cancellable = false;
            job.finished_ms = Some(crate::now_ms());
            job.rev += 1;
            let now = self.clock.now();
            table.finished.push_back((now, job.clone()));
            table.prune(now);
            job
        };
        self.events.publish(&JobUpdated(job));
    }
    /// Makes Jobs that start from now on wait, running, until `release`.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn hold(&self) {
        *self.gate.closed.lock() = true;
    }
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn release(&self) {
        *self.gate.closed.lock() = false;
        self.gate.open.notify_all();
    }
    #[cfg(any(test, feature = "test-hooks"))]
    fn pass_gate(&self) {
        let mut closed = self.gate.closed.lock();
        while *closed {
            self.gate.open.wait(&mut closed);
        }
    }
}

/// What a Job's work uses to report progress and notice cancellation.
pub struct JobHandle<K: JobKind> {
    jobs: Jobs,
    id: String,
    cancel: Arc<AtomicBool>,
    kind: PhantomData<fn() -> K>,
}
impl<K: JobKind> JobHandle<K> {
    pub fn waiting_for_confirmation(&self) { self.update(|j| j.state = JobState::WaitingForConfirmation); }
    pub fn running(&self) { self.update(|j| j.state = JobState::Running); }
    pub fn id(&self) -> &str { &self.id }
    pub fn progress(&self, progress: &K::Progress) {
        let value = serde_json::to_value(progress).unwrap();
        self.update(|j| j.progress = value);
    }
    /// Allows or refuses `DELETE /jobs/{id}` from now on.
    pub fn set_cancellable(&self, cancellable: bool) {
        self.update(|j| j.cancellable = cancellable);
    }
    /// `Err(Stop::Cancelled)` once someone cancelled the Job.
    pub fn check(&self) -> Result<(), Stop> {
        if self.cancel.load(Ordering::SeqCst) {
            Err(Stop::Cancelled)
        } else {
            Ok(())
        }
    }
    fn update(&self, change: impl FnOnce(&mut Job)) {
        self.jobs.update(&self.id, change);
    }
}

/// `job_` and 16 random lowercase letters and digits.
fn new_id() -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the OS random source works");
    let tail: String = bytes
        .iter()
        .map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char)
        .collect();
    format!("job_{tail}")
}
