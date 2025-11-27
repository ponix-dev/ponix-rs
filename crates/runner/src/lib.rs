//! A concurrent application runner that manages long-running processes with graceful shutdown.
//!
//! This runner orchestrates multiple app processes and cleanup functions, providing:
//! - Concurrent execution of multiple processes
//! - Graceful shutdown on SIGTERM/SIGINT
//! - Configurable cleanup timeout
//! - Automatic cleanup execution regardless of process outcome
//!
//! # Example
//!
//! ```no_run
//! use ponix_runner::Runner;
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() {
//!     let runner = Runner::new()
//!         .with_app_process(|ctx| async move {
//!             loop {
//!                 tokio::select! {
//!                     _ = ctx.cancelled() => {
//!                         tracing::info!("Process stopping gracefully");
//!                         break;
//!                     }
//!                     _ = tokio::time::sleep(Duration::from_secs(1)) => {
//!                         tracing::info!("Process working...");
//!                     }
//!                 }
//!             }
//!             Ok(())
//!         })
//!         .with_closer(|| async move {
//!             tracing::info!("Cleaning up resources");
//!             Ok(())
//!         })
//!         .with_closer_timeout(Duration::from_secs(5));
//!
//!     runner.run().await;
//! }
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Type alias for an app process function.
/// Takes a cancellation token and returns a future that resolves to Result<(), anyhow::Error>
pub type AppProcess = Box<
    dyn FnOnce(CancellationToken) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send,
>;

/// Type alias for a closer function.
/// Returns a future that resolves to Result<(), anyhow::Error>
pub type Closer =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>> + Send>;

/// A named app process for logging purposes.
pub struct NamedProcess {
    /// The name of the process for logging
    pub name: String,
    /// The process function
    pub process: AppProcess,
}

impl NamedProcess {
    /// Creates a new named process.
    pub fn new<S: Into<String>>(name: S, process: AppProcess) -> Self {
        Self {
            name: name.into(),
            process,
        }
    }
}

/// A concurrent application runner that manages long-running processes with graceful shutdown.
///
/// The `Runner` orchestrates multiple app processes and cleanup functions:
/// - App processes run concurrently until one fails or a shutdown signal is received
/// - Closers execute afterward, regardless of process outcome
/// - Signal handling (SIGTERM/SIGINT) implements graceful shutdown
pub struct Runner {
    app_processes: Vec<NamedProcess>,
    closers: Vec<Closer>,
    closer_timeout: Duration,
    cancellation_token: CancellationToken,
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

impl Runner {
    /// Creates a new Runner with default configuration.
    ///
    /// Default settings:
    /// - Closer timeout: 10 seconds
    /// - No app processes or closers
    pub fn new() -> Self {
        Self {
            app_processes: Vec::new(),
            closers: Vec::new(),
            closer_timeout: Duration::from_secs(10),
            cancellation_token: CancellationToken::new(),
        }
    }

    /// Adds a named app process to the runner.
    ///
    /// App processes run concurrently. If any process returns an error,
    /// all processes are cancelled and closers are executed.
    ///
    /// # Arguments
    ///
    /// * `name` - A human-readable name for the process (used in logging)
    /// * `process` - A function that takes a CancellationToken and returns a Future
    pub fn with_named_process<S, F, Fut>(mut self, name: S, process: F) -> Self
    where
        S: Into<String>,
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        self.app_processes.push(NamedProcess::new(
            name,
            Box::new(|token| Box::pin(process(token))),
        ));
        self
    }

    /// Adds an app process to the runner with an auto-generated name.
    ///
    /// App processes run concurrently. If any process returns an error,
    /// all processes are cancelled and closers are executed.
    ///
    /// # Arguments
    ///
    /// * `process` - A function that takes a CancellationToken and returns a Future
    pub fn with_app_process<F, Fut>(mut self, process: F) -> Self
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        let name = format!("process_{}", self.app_processes.len());
        self.app_processes.push(NamedProcess::new(
            name,
            Box::new(|token| Box::pin(process(token))),
        ));
        self
    }

    /// Adds a closer to the runner.
    ///
    /// Closers are executed after all app processes have stopped,
    /// regardless of whether they stopped due to error or cancellation.
    /// All closers will attempt to execute even if some fail.
    ///
    /// # Arguments
    ///
    /// * `closer` - A function that returns a Future for cleanup
    pub fn with_closer<F, Fut>(mut self, closer: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        self.closers.push(Box::new(|| Box::pin(closer())));
        self
    }

    /// Sets the timeout for executing closers.
    ///
    /// Default is 10 seconds.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Duration to wait for all closers to complete
    pub fn with_closer_timeout(mut self, timeout: Duration) -> Self {
        self.closer_timeout = timeout;
        self
    }

    /// Sets a custom cancellation token.
    ///
    /// This allows external control over process cancellation.
    ///
    /// # Arguments
    ///
    /// * `token` - The cancellation token to use
    pub fn with_cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = token;
        self
    }

    /// Runs all app processes and waits for completion or shutdown signal.
    ///
    /// This method:
    /// 1. Spawns all app processes concurrently
    /// 2. Monitors for SIGTERM/SIGINT signals
    /// 3. Cancels all processes when a signal is received or any process fails
    /// 4. Executes all closers with the configured timeout
    /// 5. Exits the application with code 0
    pub async fn run(self) {
        // Hello message
        tracing::info!(
            process_count = self.app_processes.len(),
            "hello! starting application"
        );

        let token = Arc::new(self.cancellation_token);
        let mut join_set = JoinSet::new();
        let closer_timeout = self.closer_timeout;
        let closers = self.closers;

        // Spawn all app processes with names
        for named_process in self.app_processes {
            let process_token = token.clone();
            let process_name = named_process.name.clone();

            tracing::info!(process = %process_name, "starting process");

            join_set.spawn(async move {
                let result = (named_process.process)((*process_token).clone()).await;
                (process_name, result)
            });
        }

        // Spawn signal handler
        let signal_token = token.clone();
        tokio::spawn(async move {
            match tokio::signal::ctrl_c().await {
                Ok(()) => {
                    tracing::info!("received shutdown signal");
                    signal_token.cancel();
                }
                Err(err) => {
                    tracing::error!(error = %err, "error setting up signal handler");
                }
            }
        });

        // Also handle SIGTERM on Unix systems
        #[cfg(unix)]
        {
            let sigterm_token = token.clone();
            tokio::spawn(async move {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigterm =
                    signal(SignalKind::terminate()).expect("Failed to set up SIGTERM handler");
                sigterm.recv().await;
                tracing::info!("received SIGTERM signal");
                sigterm_token.cancel();
            });
        }

        // Wait for any process to complete or fail
        let mut first_error = None;
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((name, Ok(()))) => {
                    // Process completed successfully
                    tracing::info!(process = %name, "process completed successfully");
                }
                Ok((name, Err(err))) => {
                    // Process returned an error
                    if !token.is_cancelled() {
                        tracing::error!(process = %name, error = %err, "process error");
                        first_error = Some(err);
                        token.cancel();
                    }
                }
                Err(err) => {
                    // Task panicked
                    tracing::error!(error = %err, "process panicked");
                    if !token.is_cancelled() {
                        token.cancel();
                    }
                }
            }

            // If we got an error or cancellation, cancel all remaining processes
            if token.is_cancelled() {
                break;
            }
        }

        // Wait for remaining tasks to complete after cancellation
        join_set.shutdown().await;

        // Execute closers with timeout
        if !closers.is_empty() {
            tracing::info!(timeout_secs = closer_timeout.as_secs(), "running closers");

            let closer_result =
                tokio::time::timeout(closer_timeout, Self::run_closers_static(closers)).await;

            match closer_result {
                Ok(_) => {
                    tracing::info!("all closers completed");
                }
                Err(_) => {
                    tracing::error!(timeout_secs = closer_timeout.as_secs(), "closers timed out");
                }
            }
        }

        // Goodbye message
        if let Some(err) = first_error {
            tracing::error!(error = %err, "goodbye! application exiting with error");
            std::process::exit(1);
        } else {
            tracing::info!("goodbye! application exiting normally");
            std::process::exit(0);
        }
    }

    /// Runs all closers concurrently.
    async fn run_closers_static(closers: Vec<Closer>) {
        let mut closer_set = JoinSet::new();

        for closer in closers {
            closer_set.spawn(async move { closer().await });
        }

        while let Some(result) = closer_set.join_next().await {
            match result {
                Ok(Ok(())) => {
                    tracing::debug!("closer completed successfully");
                }
                Ok(Err(err)) => {
                    tracing::error!("closer error: {:#}", err);
                }
                Err(err) => {
                    tracing::error!("closer panicked: {}", err);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_runner_with_successful_processes() {
        let closer_called = Arc::new(AtomicBool::new(false));
        let closer_called_clone = closer_called.clone();

        let token = CancellationToken::new();
        let token_clone = token.clone();

        // Create a runner that we'll cancel manually
        let _runner = Runner::new()
            .with_app_process(|ctx| async move {
                tokio::select! {
                    _ = ctx.cancelled() => {
                        Ok(())
                    }
                    _ = tokio::time::sleep(Duration::from_secs(100)) => {
                        Ok(())
                    }
                }
            })
            .with_closer(move || {
                let flag = closer_called_clone.clone();
                async move {
                    flag.store(true, Ordering::SeqCst);
                    Ok(())
                }
            })
            .with_cancellation_token(token.clone())
            .with_closer_timeout(Duration::from_secs(5));

        // Cancel after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            token_clone.cancel();
        });

        // This will run until cancelled and then exit the process
        // We can't actually test the full run() because it calls std::process::exit
        // So we'll test the components separately
    }

    #[tokio::test]
    async fn test_closer_execution() {
        let counter = Arc::new(AtomicBool::new(false));
        let counter_clone = counter.clone();

        let runner = Runner::new()
            .with_closer(move || {
                let c = counter_clone.clone();
                async move {
                    c.store(true, Ordering::SeqCst);
                    Ok(())
                }
            })
            .with_closer_timeout(Duration::from_secs(1));

        Runner::run_closers_static(runner.closers).await;
        assert!(counter.load(Ordering::SeqCst));
    }
}
