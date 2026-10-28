use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

/// A thread the editor sends requests to, and polls for what it sends back,
/// so blocking work stays off the UI thread.
pub(crate) struct Worker<Req, Res> {
    name: &'static str,
    requests: mpsc::Sender<Req>,
    results: mpsc::Receiver<Res>,
}

/// What one poll of a worker took.
pub(crate) struct Polled<T> {
    pub(crate) results: Vec<T>,
    /// Why no result comes again, once the worker's thread has ended.
    pub(crate) stopped: Option<String>,
}

impl<Req: Send + 'static, Res: Send + 'static> Worker<Req, Res> {
    /// Runs `body` on a thread named `celesta-<name>`. It serves the requests
    /// as it likes, and ends once either channel closes.
    pub(crate) fn spawn(
        name: &'static str,
        body: impl FnOnce(mpsc::Receiver<Req>, mpsc::Sender<Res>) + Send + 'static,
    ) -> anyhow::Result<Self> {
        let (request_tx, request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        thread::Builder::new()
            .name(format!("celesta-{}", name.replace(' ', "-")))
            .spawn(move || body(request_rx, result_tx))?;
        Ok(Self {
            name,
            requests: request_tx,
            results: result_rx,
        })
    }
}

impl<Req, Res> Worker<Req, Res> {
    pub(crate) fn request(&self, request: Req) -> Result<(), String> {
        self.requests.send(request).map_err(|_| self.stopped())
    }

    /// Takes every result that arrived, without waiting. `stopped` is set on
    /// every poll after the thread ended; the caller knows whether it still
    /// waited for something.
    pub(crate) fn poll(&self) -> Polled<Res> {
        let mut results = Vec::new();
        loop {
            match self.results.try_recv() {
                Ok(result) => results.push(result),
                Err(mpsc::TryRecvError::Empty) => {
                    return Polled {
                        results,
                        stopped: None,
                    };
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Polled {
                        results,
                        stopped: Some(self.stopped()),
                    };
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Res, mpsc::RecvTimeoutError> {
        self.results.recv_timeout(timeout)
    }

    fn stopped(&self) -> String {
        format!("{} worker stopped unexpectedly", self.name)
    }
}

/// A [`Worker`] for which only the newest request matters: requests queued
/// while it is busy coalesce to the newest, and each is numbered with a
/// generation so the result of a superseded one is dropped.
pub(crate) struct LatestWorker<Req, Res> {
    worker: Worker<(Job, Req), (u64, Res)>,
    generation: u64,
    /// `generation`, as running jobs read it.
    newest: Arc<AtomicU64>,
    /// A request of the current generation has no result yet.
    pending: bool,
}

/// The request a [`LatestWorker`] is serving.
pub(crate) struct Job {
    generation: u64,
    newest: Arc<AtomicU64>,
}

impl Job {
    /// Whether a newer request superseded this one, whose result would be
    /// dropped: long work checks this to stop early.
    pub(crate) fn superseded(&self) -> bool {
        self.newest.load(Ordering::Acquire) != self.generation
    }
}

impl<Req: Send + 'static, Res: Send + 'static> LatestWorker<Req, Res> {
    /// Serves the newest queued request with `serve`, one at a time.
    pub(crate) fn spawn(
        name: &'static str,
        mut serve: impl FnMut(Req, &Job) -> Res + Send + 'static,
    ) -> anyhow::Result<Self> {
        let worker = Worker::spawn(name, move |requests, results| {
            while let Ok(first) = requests.recv() {
                let (job, request) = take_latest(first, &requests);
                let result = serve(request, &job);
                if results.send((job.generation, result)).is_err() {
                    break;
                }
            }
        })?;
        Ok(Self {
            worker,
            generation: 0,
            newest: Arc::new(AtomicU64::new(0)),
            pending: false,
        })
    }
}

impl<Req, Res> LatestWorker<Req, Res> {
    /// Starts a new generation: every earlier request is superseded, and
    /// nothing is pending until [`request`](Self::request).
    pub(crate) fn supersede(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.newest.store(self.generation, Ordering::Release);
        self.pending = false;
    }

    /// Sends `request` in the current generation; it is pending until its
    /// result is polled.
    pub(crate) fn request(&mut self, request: Req) -> Result<(), String> {
        let job = Job {
            generation: self.generation,
            newest: Arc::clone(&self.newest),
        };
        let sent = self.worker.request((job, request));
        self.pending = sent.is_ok();
        sent
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) const fn pending(&self) -> bool {
        self.pending
    }

    /// Takes the results of the current generation. `stopped` is set once,
    /// when the thread ended with a request pending.
    pub(crate) fn poll(&mut self) -> Polled<Res> {
        let current = self.generation;
        let Polled { results, stopped } = self.poll_where(|generation| generation == current);
        Polled {
            results: results.into_iter().map(|(_, result)| result).collect(),
            stopped,
        }
    }

    /// Like [`poll`](Self::poll), but takes every result newer than
    /// generation `shown`, with its generation: when the work is slower than
    /// requests come, the newest request is always still running, and
    /// waiting for its result would show nothing.
    pub(crate) fn poll_newer_than(&mut self, shown: u64) -> Polled<(u64, Res)> {
        self.poll_where(|generation| generation > shown)
    }

    fn poll_where(&mut self, keep: impl Fn(u64) -> bool) -> Polled<(u64, Res)> {
        let Polled { results, stopped } = self.worker.poll();
        if results
            .iter()
            .any(|(generation, _)| *generation == self.generation)
        {
            self.pending = false;
        }
        Polled {
            results: results
                .into_iter()
                .filter(|(generation, _)| keep(*generation))
                .collect(),
            stopped: stopped.filter(|_| std::mem::take(&mut self.pending)),
        }
    }
}

/// `latest`, or the last of the requests queued behind it.
pub(crate) fn take_latest<T>(mut latest: T, receiver: &mpsc::Receiver<T>) -> T {
    while let Ok(next) = receiver.try_recv() {
        latest = next;
    }
    latest
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Polls `worker` until a poll takes something or reports it stopped.
    fn poll_until_answered<Req, Res>(worker: &mut LatestWorker<Req, Res>) -> Polled<Res> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let polled = worker.poll();
            if !polled.results.is_empty() || polled.stopped.is_some() {
                return polled;
            }
            assert!(Instant::now() < deadline, "the worker answered nothing");
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// A worker that answers each number with itself once `gate` lets it,
    /// telling `started` which number it began.
    fn gated() -> (
        LatestWorker<u32, u32>,
        mpsc::Sender<()>,
        mpsc::Receiver<u32>,
    ) {
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let (started_tx, started_rx) = mpsc::channel();
        let worker = LatestWorker::spawn("test", move |request: u32, _: &Job| {
            started_tx.send(request).unwrap();
            gate_rx.recv().unwrap();
            request
        })
        .unwrap();
        (worker, gate_tx, started_rx)
    }

    #[test]
    fn queued_requests_coalesce_to_the_newest() {
        let (sender, receiver) = mpsc::channel();
        sender.send(2).unwrap();
        sender.send(3).unwrap();

        assert_eq!(take_latest(1, &receiver), 3);
    }

    #[test]
    fn only_the_newest_request_queued_behind_a_running_one_is_served() {
        let (mut worker, gate, started) = gated();
        for request in 1..=3 {
            worker.supersede();
            worker.request(request).unwrap();
            if request == 1 {
                assert_eq!(started.recv().unwrap(), 1);
            }
        }
        gate.send(()).unwrap();
        gate.send(()).unwrap();

        // The first one's result is of a superseded generation.
        let polled = poll_until_answered(&mut worker);
        assert_eq!(polled.results, [3]);
        assert_eq!(started.try_iter().collect::<Vec<_>>(), [3]);
        assert!(!worker.pending());
    }

    #[test]
    fn a_running_job_sees_a_newer_request() {
        let (superseded_tx, superseded_rx) = mpsc::channel();
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let mut worker = LatestWorker::spawn("test", move |(): (), job: &Job| {
            superseded_tx.send(job.superseded()).unwrap();
            gate_rx.recv().unwrap();
            superseded_tx.send(job.superseded()).unwrap();
        })
        .unwrap();
        worker.supersede();
        worker.request(()).unwrap();
        assert!(!superseded_rx.recv().unwrap());

        worker.supersede();
        gate_tx.send(()).unwrap();
        assert!(superseded_rx.recv().unwrap());
        // Nothing was requested since: nothing is pending.
        assert!(!worker.pending());
    }

    #[test]
    fn results_newer_than_the_shown_one_are_all_taken() {
        let (mut worker, gate, started) = gated();
        worker.supersede();
        worker.request(1).unwrap();
        assert_eq!(started.recv().unwrap(), 1);
        worker.supersede();
        worker.request(2).unwrap();
        gate.send(()).unwrap();
        assert_eq!(started.recv().unwrap(), 2);

        // Generation 1's result is stale for `poll`, but newer than 0.
        let deadline = Instant::now() + Duration::from_secs(5);
        let polled = loop {
            let polled = worker.poll_newer_than(0);
            if !polled.results.is_empty() {
                break polled;
            }
            assert!(Instant::now() < deadline, "the worker answered nothing");
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(polled.results, [(1, 1)]);
        // The newest request is still running.
        assert!(worker.pending());

        gate.send(()).unwrap();
        assert_eq!(poll_until_answered(&mut worker).results, [2]);
        assert!(!worker.pending());
    }

    #[test]
    fn a_worker_that_stopped_with_a_request_pending_says_so_once() {
        let mut worker = LatestWorker::<(), ()>::spawn("test", |(), _| {
            panic!("the worker failed");
        })
        .unwrap();
        worker.supersede();
        worker.request(()).unwrap();

        let polled = poll_until_answered(&mut worker);
        assert!(polled.results.is_empty());
        assert_eq!(
            polled.stopped.as_deref(),
            Some("test worker stopped unexpectedly")
        );
        assert!(!worker.pending());
        assert_eq!(worker.poll().stopped, None);
        assert_eq!(
            worker.request(()),
            Err("test worker stopped unexpectedly".to_owned())
        );
        assert!(!worker.pending());
    }

    #[test]
    fn a_worker_reports_every_poll_after_it_stopped() {
        let worker = Worker::<(), ()>::spawn("test", |_, _| {}).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while worker.poll().stopped.is_none() {
            assert!(Instant::now() < deadline, "the worker did not stop");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            worker.poll().stopped.as_deref(),
            Some("test worker stopped unexpectedly")
        );
    }
}
