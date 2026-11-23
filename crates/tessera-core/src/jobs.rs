//! A deduplicating worker pool with two priorities.
//!
//! High-priority jobs come from requests for something on screen right now and run newest
//! first (LIFO): when the user scrolls, what they are looking at now beats what they
//! scrolled past. Low-priority jobs are background work and run oldest first. A high request
//! for a job already queued as low promotes it. If every waiter of a high job goes away
//! before it starts (e.g. the browser cancelled the image request), the job is dropped or
//! demoted back to low if background work also wanted it.
//!
//! Workers are OS threads; job functions are blocking (they run ffmpeg & co.). Callers can
//! wait for a job asynchronously ([`Pool::wait`], [`Job::done`]) or by blocking
//! ([`Job::wait_blocking`]).

use std::collections::{HashMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low,
    High,
}

/// Why a job did not complete successfully.
#[derive(Debug, Clone, thiserror::Error)]
pub enum JobError {
    /// Nobody waited for it anymore before it started.
    #[error("job dropped")]
    Dropped,
    /// The pool shut down (or the caller stopped waiting).
    #[error("context canceled")]
    Cancelled,
    /// The job function returned an error (remembered for [`Pool::failure_ttl`]).
    #[error("{0}")]
    Failed(Arc<anyhow::Error>),
}

impl JobError {
    pub fn failed(msg: impl std::fmt::Display) -> JobError {
        JobError::Failed(Arc::new(anyhow::anyhow!("{msg}")))
    }
}

pub type JobResult = Result<(), JobError>;

/// The work of a job. Runs once on a worker thread.
pub type RunFn = Box<dyn FnOnce(&JobCtx) -> anyhow::Result<()> + Send + 'static>;

/// What a running job sees: its handle (progress) and the pool's cancellation token.
pub struct JobCtx {
    pub job: Arc<Job>,
    pub cancel: CancellationToken,
}

impl JobCtx {
    /// Whether the job started as low-priority background work (callers run such work at
    /// a lower CPU priority so it yields to everything else on the machine).
    pub fn background(&self) -> bool {
        self.job.background()
    }
    pub fn set_progress(&self, f: f64) {
        self.job.set_progress(f)
    }
}

/// A queued, running or finished job.
pub struct Job {
    pub key: String,
    progress: AtomicI64,
    bg: AtomicBool,
    result: Mutex<Option<JobResult>>,
    cv: Condvar,
    notify: tokio::sync::Notify,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job").field("key", &self.key).finish()
    }
}

impl Job {
    fn new(key: &str) -> Arc<Job> {
        Arc::new(Job {
            key: key.to_string(),
            progress: AtomicI64::new(0),
            bg: AtomicBool::new(false),
            result: Mutex::new(None),
            cv: Condvar::new(),
            notify: tokio::sync::Notify::new(),
        })
    }

    /// Whether the job started as background (low priority) work.
    pub fn background(&self) -> bool {
        self.bg.load(Ordering::SeqCst)
    }

    /// Reports progress in [0,1].
    pub fn set_progress(&self, f: f64) {
        let f = f.clamp(0.0, 1.0);
        self.progress.store((f * 1000.0) as i64, Ordering::SeqCst);
    }

    pub fn progress(&self) -> f64 {
        self.progress.load(Ordering::SeqCst) as f64 / 1000.0
    }

    /// The result if the job has finished.
    pub fn err(&self) -> Option<JobError> {
        self.result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .and_then(|r| r.err())
    }

    pub fn is_done(&self) -> bool {
        self.result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    fn complete(&self, r: JobResult) {
        let mut g = self.result.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_none() {
            *g = Some(r);
        }
        drop(g);
        self.cv.notify_all();
        self.notify.notify_waiters();
    }

    /// Waits (async) until the job finishes.
    pub async fn done(&self) -> JobResult {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(r) = self
                .result
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
            {
                return r;
            }
            notified.await;
        }
    }

    /// Blocks the current thread until the job finishes.
    pub fn wait_blocking(&self) -> JobResult {
        let mut g = self.result.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(r) = g.clone() {
                return r;
            }
            g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Blocks until the job finishes or `timeout` passes (`None` = still running).
    pub fn wait_timeout(&self, timeout: Duration) -> Option<JobResult> {
        let deadline = Instant::now() + timeout;
        let mut g = self.result.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(r) = g.clone() {
                return Some(r);
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            g = self
                .cv
                .wait_timeout(g, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum St {
    Queued,
    Running,
}

struct JobState {
    job: Arc<Job>,
    run: Option<RunFn>,
    prio: Priority,
    /// Background generation also wants this.
    want_low: bool,
    waiters: i64,
    st: St,
    /// Position in the list it is queued in (lazy deletion).
    seq: u64,
    started: Instant,
}

struct Entry {
    key: String,
    seq: u64,
}

struct Inner {
    high: Vec<Entry>,
    low: VecDeque<Entry>,
    jobs: HashMap<String, JobState>,
    failed: HashMap<String, (JobError, Instant)>,
    seq: u64,
    running: i64,
    closed: bool,
}

struct Shared {
    name: String,
    workers: usize,
    failure_ttl: Duration,
    inner: Mutex<Inner>,
    cond: Condvar,
    completed: AtomicI64,
    failures: AtomicI64,
    busy_ns: AtomicU64,
    cancel: CancellationToken,
}

/// The worker pool.
pub struct Pool {
    shared: Arc<Shared>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

/// Pool counters for the status page.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Stats {
    pub name: String,
    pub workers: usize,
    pub running: i64,
    #[serde(rename = "queuedHigh")]
    pub high: i64,
    #[serde(rename = "queuedLow")]
    pub low: i64,
    pub completed: i64,
    pub failed: i64,
    #[serde(rename = "avgMs")]
    pub avg_ms: f64,
}

/// A running job (progress display).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActiveJob {
    pub key: String,
    pub progress: f64,
    pub seconds: f64,
}

/// How long a failed job's error is remembered (no retry storm for broken files).
pub const FAILURE_TTL: Duration = Duration::from_secs(10 * 60);

impl Pool {
    pub fn new(name: &str, workers: usize) -> Pool {
        Self::with_failure_ttl(name, workers, FAILURE_TTL)
    }

    pub fn with_failure_ttl(name: &str, workers: usize, failure_ttl: Duration) -> Pool {
        let workers = workers.max(1);
        let shared = Arc::new(Shared {
            name: name.to_string(),
            workers,
            failure_ttl,
            inner: Mutex::new(Inner {
                high: Vec::new(),
                low: VecDeque::new(),
                jobs: HashMap::new(),
                failed: HashMap::new(),
                seq: 0,
                running: 0,
                closed: false,
            }),
            cond: Condvar::new(),
            completed: AtomicI64::new(0),
            failures: AtomicI64::new(0),
            busy_ns: AtomicU64::new(0),
            cancel: CancellationToken::new(),
        });
        let threads = (0..workers)
            .map(|i| {
                let s = shared.clone();
                std::thread::Builder::new()
                    .name(format!("{name}-{i}"))
                    .spawn(move || worker(s))
                    .expect("spawn pool worker")
            })
            .collect();
        Pool {
            shared,
            threads: Mutex::new(threads),
        }
    }

    pub fn name(&self) -> &str {
        &self.shared.name
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.shared.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Stops the workers (running jobs see their cancellation token fire) and waits for
    /// them to exit.
    pub fn close(&self) {
        self.lock().closed = true;
        self.shared.cancel.cancel();
        self.shared.cond.notify_all();
        let threads: Vec<_> =
            std::mem::take(&mut *self.threads.lock().unwrap_or_else(|e| e.into_inner()));
        for t in threads {
            let _ = t.join();
        }
        // anything still queued will never run
        let mut g = self.lock();
        for (_, js) in g.jobs.drain() {
            js.job.complete(Err(JobError::Cancelled));
        }
    }

    /// Queues a job unless one with the same key is already queued or running, in which case
    /// the existing job is returned (and promoted if `prio` is higher). If `waiting` is true
    /// the caller counts as a waiter and must call [`Pool::release`] when it stops waiting.
    pub fn submit(
        &self,
        key: &str,
        prio: Priority,
        waiting: bool,
        run: impl FnOnce(&JobCtx) -> anyhow::Result<()> + Send + 'static,
    ) -> Result<Arc<Job>, JobError> {
        let mut g = self.lock();
        if let Some((err, at)) = g.failed.get(key) {
            if at.elapsed() < self.shared.failure_ttl {
                return Err(err.clone());
            }
            g.failed.remove(key);
        }
        if g.closed {
            return Err(JobError::Cancelled);
        }
        let inner = &mut *g;
        let (job, enqueue) = match inner.jobs.get_mut(key) {
            None => {
                let job = Job::new(key);
                inner.jobs.insert(
                    key.to_string(),
                    JobState {
                        job: job.clone(),
                        run: Some(Box::new(run)),
                        prio,
                        want_low: false,
                        waiters: 0,
                        st: St::Queued,
                        seq: 0,
                        started: Instant::now(),
                    },
                );
                (job, true)
            }
            Some(js) => {
                let promote = js.st == St::Queued && prio > js.prio;
                if promote {
                    js.prio = prio;
                }
                (js.job.clone(), promote)
            }
        };
        if enqueue {
            self.enqueue(inner, key);
        }
        if let Some(js) = inner.jobs.get_mut(key) {
            if prio == Priority::Low {
                js.want_low = true;
            }
            if waiting {
                js.waiters += 1;
            }
        }
        Ok(job)
    }

    /// Moves a queued background job to the front (high priority) without anyone waiting
    /// for it, e.g. to get the next presentation slides ready.
    pub fn promote(&self, key: &str) {
        let mut g = self.lock();
        let inner = &mut *g;
        if let Some(js) = inner.jobs.get_mut(key) {
            if js.st == St::Queued && js.prio != Priority::High {
                js.prio = Priority::High;
                js.want_low = true;
                self.enqueue(inner, key);
            }
        }
    }

    fn enqueue(&self, inner: &mut Inner, key: &str) {
        inner.seq += 1;
        let seq = inner.seq;
        let Some(js) = inner.jobs.get_mut(key) else {
            return;
        };
        js.seq = seq;
        let e = Entry {
            key: key.to_string(),
            seq,
        };
        if js.prio == Priority::High {
            inner.high.push(e);
        } else {
            inner.low.push_back(e);
        }
        self.shared.cond.notify_one();
    }

    /// Drops a waiter. A high job that nobody waits for anymore and hasn't started is
    /// demoted to low (if background work wants it) or dropped.
    pub fn release(&self, job: &Arc<Job>) {
        let mut g = self.lock();
        let inner = &mut *g;
        let Some(js) = inner.jobs.get_mut(&job.key) else {
            return;
        };
        if !Arc::ptr_eq(&js.job, job) {
            return;
        }
        js.waiters -= 1;
        if js.waiters > 0 || js.st != St::Queued || js.prio != Priority::High {
            return;
        }
        if js.want_low {
            js.prio = Priority::Low;
            let key = job.key.clone();
            self.enqueue(inner, &key);
            return;
        }
        if let Some(js) = inner.jobs.remove(&job.key) {
            js.job.complete(Err(JobError::Dropped));
        }
    }

    /// Submits (or joins) a high-priority job and waits until it finishes. Dropping the
    /// returned future stops waiting (and releases the job like a cancelled request).
    pub async fn wait(
        &self,
        key: &str,
        run: impl FnOnce(&JobCtx) -> anyhow::Result<()> + Send + 'static,
    ) -> JobResult {
        let job = self.submit(key, Priority::High, true, run)?;
        struct Guard<'a> {
            pool: &'a Pool,
            job: Arc<Job>,
        }
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.pool.release(&self.job);
            }
        }
        let guard = Guard { pool: self, job };
        guard.job.done().await
    }

    /// A queued/running job by key, if any.
    pub fn get(&self, key: &str) -> Option<Arc<Job>> {
        self.lock().jobs.get(key).map(|js| js.job.clone())
    }

    /// Whether the job with `key` is running right now.
    pub fn is_running(&self, key: &str) -> bool {
        self.lock()
            .jobs
            .get(key)
            .map(|js| js.st == St::Running)
            .unwrap_or(false)
    }

    /// A remembered failure for `key`.
    pub fn last_error(&self, key: &str) -> Option<JobError> {
        let g = self.lock();
        g.failed
            .get(key)
            .filter(|(_, at)| at.elapsed() < self.shared.failure_ttl)
            .map(|(e, _)| e.clone())
    }

    /// Forgets a failure so the job can be retried.
    pub fn clear_failure(&self, key: &str) {
        self.lock().failed.remove(key);
    }

    pub fn stats(&self) -> Stats {
        let g = self.lock();
        let completed = self.shared.completed.load(Ordering::SeqCst);
        let failed = self.shared.failures.load(Ordering::SeqCst);
        let mut s = Stats {
            name: self.shared.name.clone(),
            workers: self.shared.workers,
            running: g.running,
            high: 0,
            low: 0,
            completed,
            failed,
            avg_ms: 0.0,
        };
        for js in g.jobs.values() {
            if js.st == St::Queued {
                if js.prio == Priority::High {
                    s.high += 1;
                } else {
                    s.low += 1;
                }
            }
        }
        let n = completed + failed;
        if n > 0 {
            s.avg_ms = self.shared.busy_ns.load(Ordering::SeqCst) as f64 / n as f64 / 1e6;
        }
        s
    }

    /// Running jobs with their progress.
    pub fn active(&self) -> Vec<ActiveJob> {
        let g = self.lock();
        g.jobs
            .values()
            .filter(|js| js.st == St::Running)
            .map(|js| ActiveJob {
                key: js.job.key.clone(),
                progress: js.job.progress(),
                seconds: js.started.elapsed().as_secs_f64(),
            })
            .collect()
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.close();
    }
}

fn worker(s: Arc<Shared>) {
    loop {
        let mut g = s.inner.lock().unwrap_or_else(|e| e.into_inner());
        let key = loop {
            let mut found = None;
            while let Some(e) = g.high.pop() {
                if let Some(js) = g.jobs.get(&e.key) {
                    if js.st == St::Queued && js.seq == e.seq {
                        found = Some(e.key);
                        break;
                    }
                }
            }
            if found.is_none() {
                while let Some(e) = g.low.pop_front() {
                    if let Some(js) = g.jobs.get(&e.key) {
                        if js.st == St::Queued && js.seq == e.seq {
                            found = Some(e.key);
                            break;
                        }
                    }
                }
            }
            if let Some(k) = found {
                break Some(k);
            }
            if g.closed {
                break None;
            }
            g = s.cond.wait(g).unwrap_or_else(|e| e.into_inner());
        };
        let Some(key) = key else { return };
        let (job, run) = {
            let Some(js) = g.jobs.get_mut(&key) else {
                continue;
            };
            js.st = St::Running;
            js.started = Instant::now();
            js.job.bg.store(js.prio == Priority::Low, Ordering::SeqCst);
            (js.job.clone(), js.run.take())
        };
        g.running += 1;
        drop(g);

        let started = Instant::now();
        let ctx = JobCtx {
            job: job.clone(),
            cancel: s.cancel.clone(),
        };
        let res: anyhow::Result<()> = match run {
            Some(f) => match catch_unwind(AssertUnwindSafe(|| f(&ctx))) {
                Ok(r) => r,
                Err(_) => Err(anyhow::anyhow!("panic in job")),
            },
            None => Ok(()),
        };
        s.busy_ns
            .fetch_add(started.elapsed().as_nanos() as u64, Ordering::SeqCst);

        let mut g = s.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.running -= 1;
        if g.jobs
            .get(&key)
            .map(|js| Arc::ptr_eq(&js.job, &job))
            .unwrap_or(false)
        {
            g.jobs.remove(&key);
        }
        let result = match res {
            Ok(()) => {
                s.completed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                s.failures.fetch_add(1, Ordering::SeqCst);
                let err = if s.cancel.is_cancelled() {
                    JobError::Cancelled
                } else {
                    JobError::Failed(Arc::new(e))
                };
                if !s.cancel.is_cancelled() {
                    g.failed.insert(key.clone(), (err.clone(), Instant::now()));
                }
                Err(err)
            }
        };
        job.complete(result);
        drop(g);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI32;

    #[tokio::test(flavor = "multi_thread")]
    async fn dedup_and_wait() {
        let p = Arc::new(Pool::new("t", 2));
        let runs = Arc::new(AtomicI32::new(0));
        let mut handles = Vec::new();
        for _ in 0..10 {
            let p = p.clone();
            let runs = runs.clone();
            handles.push(tokio::spawn(async move {
                p.wait("same", move |_| {
                    runs.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(30));
                    Ok(())
                })
                .await
            }));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1, "job ran more than once");
    }

    #[test]
    fn high_before_low_and_lifo() {
        let p = Pool::new("t", 1);
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let order = Arc::new(Mutex::new(Vec::<String>::new()));
        let rec = |name: &'static str| {
            let order = order.clone();
            move |_: &JobCtx| {
                order.lock().unwrap().push(name.to_string());
                Ok(())
            }
        };
        // occupy the only worker
        p.submit("blocker", Priority::High, false, move |_| {
            let _ = rx.recv();
            Ok(())
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(20));
        p.submit("low1", Priority::Low, false, rec("low1")).unwrap();
        p.submit("low2", Priority::Low, false, rec("low2")).unwrap();
        p.submit("high1", Priority::High, false, rec("high1"))
            .unwrap();
        p.submit("high2", Priority::High, false, rec("high2"))
            .unwrap();
        let j = p.submit("last", Priority::Low, false, rec("last")).unwrap();
        drop(tx);
        j.wait_blocking().unwrap();
        let want = ["high2", "high1", "low1", "low2", "last"];
        assert_eq!(*order.lock().unwrap(), want);
    }

    #[test]
    fn promote_and_drop_on_release() {
        let p = Pool::new("t", 1);
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        p.submit("blocker", Priority::High, false, move |_| {
            let _ = rx.recv();
            Ok(())
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let ran = Arc::new(AtomicBool::new(false));
        let r1 = ran.clone();
        let j = p
            .submit("x", Priority::High, true, move |_| {
                r1.store(true, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        p.release(&j); // nobody waits anymore and no background interest → dropped
        assert!(matches!(j.err(), Some(JobError::Dropped)), "want dropped");
        // background interest keeps it alive (demoted)
        let r2 = ran.clone();
        let j2 = p
            .submit("y", Priority::Low, false, move |_| {
                r2.store(true, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        p.submit("y", Priority::High, true, |_| Ok(())).unwrap();
        p.release(&j2);
        drop(tx);
        assert!(j2.wait_blocking().is_ok(), "demoted job should still run");
        assert!(ran.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn failure_remembered() {
        let p = Pool::new("t", 1);
        let err = p.wait("f", |_| Err(anyhow::anyhow!("boom"))).await;
        assert!(matches!(&err, Err(JobError::Failed(e)) if e.to_string() == "boom"));
        let again = p.submit("f", Priority::High, false, |_| Ok(()));
        assert!(
            matches!(&again, Err(JobError::Failed(e)) if e.to_string() == "boom"),
            "failure should be remembered"
        );
        p.clear_failure("f");
        p.wait("f", |_| Ok(())).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dropped_waiter_releases() {
        let p = Arc::new(Pool::new("t", 1));
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        p.submit("blocker", Priority::High, false, move |_| {
            let _ = rx.recv();
            Ok(())
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        let ran = Arc::new(AtomicBool::new(false));
        let r = ran.clone();
        let res = tokio::time::timeout(
            Duration::from_millis(30),
            p.wait("z", move |_| {
                r.store(true, Ordering::SeqCst);
                Ok(())
            }),
        )
        .await;
        assert!(res.is_err(), "should time out while blocked");
        assert!(p.get("z").is_none(), "abandoned job should be dropped");
        drop(tx);
        std::thread::sleep(Duration::from_millis(30));
        assert!(!ran.load(Ordering::SeqCst));
    }
}
