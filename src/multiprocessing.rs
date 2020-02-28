// Taken from: https://github.com/s3rvac/blog/tree/master/en-2018-04-16-implementing-multiprocessing-pool-threadpool-from-python-in-rust/

use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
};

/// A thread pool with an interface similar to that of
/// `multiprocessing.pool.ThreadPool` from Python.
pub struct ThreadPool {
    /// Internal representation of the thread pool.
    pool: threadpool::ThreadPool,
}

impl ThreadPool {
    /// Creates a new thread pool, where the number of worker threads is
    /// detected automatically based on the number of available CPUs in the
    /// system.
    pub fn new() -> Self {
        let worker_count = num_cpus::get();
        ThreadPool::with_workers(worker_count)
    }

    /// Creates a new thread pool with the given number of worker threads.
    ///
    /// # Panics
    ///
    /// When `count` is negative or zero.
    pub fn with_workers(count: usize) -> Self {
        assert!(count > 0, format!("worker count cannot be {}", count));

        ThreadPool {
            pool: threadpool::ThreadPool::new(count),
        }
    }

    /// Returns the number of workers in the pool.
    pub fn worker_count(&self) -> usize {
        self.pool.max_count()
    }

    /// Applies `f` to all values in `inputs` in parallel and returns an
    /// iterator over the results.
    ///
    /// The order of the returned results matches the order of inputs. That is,
    /// if you pass it the increment function and `[1, 2, 3]`, you will always
    /// get the results in this order: `2`, `3`, `4`.
    pub fn imap<F, I, T, R>(&self, f: F, inputs: I) -> IMapIterator<R>
        where F: Fn(T) -> R + Send + Sync + 'static,
              I: IntoIterator<Item = T>,
              T: Send + 'static,
              R: Send + 'static,
    {
        // We need to wrap the function with Arc so it can be passed to
        // multiple threads.
        let f = Arc::new(f);

        // We use a multiple-producers single-consumer (MPSC) channel to
        // transfer the results from the thread pool to the user.
        let (tx, rx) = mpsc::channel();

        // Submit `f(input)` for computation in the thread pool, for each
        // input. We need to keep the total count of submitted computations so
        // we know when to stop reading results from the channel in
        // IMapIterator::next().
        let mut total = 0;
        for (i, input) in inputs.into_iter().enumerate() {
            total += 1;
            let f = f.clone();
            let tx = tx.clone();
            self.pool.execute(move || {
                let result = f(input);
                if let Err(_) = tx.send((i, result)) {
                    // Sending of the result has failed, which means that the
                    // receiving side has hung up. There is nothing to do but
                    // to ignore the result.
                }
            });
        }
        IMapIterator::new(rx, total)
    }
}

/// An iterator over results from `ThreadPool::imap()`.
pub struct IMapIterator<T> {
    /// The receiving end of the channel created in `ThreadPool::imap()`.
    rx: mpsc::Receiver<(usize, T)>,

    /// As `imap()` preserves the order of the returned results (in contrast to
    /// `imap_unordered()`), we need a mapping of "indexes" into the original
    /// input iterator to their corresponding results.
    results: BTreeMap<usize, T>,

    /// Index of the next result that we should yield in `next()`.
    next: usize,

    /// The total number of results that should be yielded (so we know when to
    /// stop reading results from the channel.
    total: usize,
}

impl<T> IMapIterator<T> {
    /// Creates a new iterator.
    fn new(rx: mpsc::Receiver<(usize, T)>, total: usize) -> Self {
        IMapIterator {
            rx: rx,
            results: BTreeMap::new(),
            next: 0,
            total: total,
        }
    }
}

impl<T> Iterator for IMapIterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.total {
            // Do we already have a result for the next index?
            if let Some(result) = self.results.remove(&self.next) {
                self.next += 1;
                return Some(result);
            }

            // We do not, so try receiving the next result.
            let (i, result) = match self.rx.recv() {
                Ok((i, result)) => (i, result),
                Err(_) => {
                    // Receiving has failed, which means that the sending side
                    // has hung up. There will be no more results.
                    self.next = self.total;
                    break;
                },
            };
            assert!(i >= self.next, format!("got {}, next is {}", i, self.next));
            assert!(!self.results.contains_key(&i), format!("{} already exists", i));
            self.results.insert(i, result);
        }
        None
    }
}