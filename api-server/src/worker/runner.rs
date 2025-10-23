// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::event::PaymentProcessorEvent;
use crate::wallet::Wallet;
use crate::worker::context::JobContext;
use crate::worker::{jobs, JobResult};
use log::*;
use ootle_payment_processor_storage::models::{JobStatus, JobType};
use ootle_payment_processor_storage::{
    AsReadable, ReadableStore, StoreReadTransaction, StoreWriteTransaction, WriteableStore,
};
use std::collections::HashMap;
use std::future::poll_fn;
use std::ops::ControlFlow;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

const MAX_CONCURRENT_JOBS: usize = 10;

pub struct TaskWorker<TStore> {
    store: TStore,
    wallet: Wallet,
    notifications: broadcast::Receiver<PaymentProcessorEvent>,
    interval: tokio::time::Interval,
    work_queue: futures_bounded::FuturesMap<uuid::Uuid, anyhow::Result<JobResult>>,
    timers: HashMap<uuid::Uuid, Instant>,
}

impl<TStore> TaskWorker<TStore>
where
    TStore: ReadableStore + WriteableStore + Clone + Send + Sync + 'static,
    for<'tx> TStore::ReadTransaction<'tx>: Send,
    for<'tx> TStore::WriteTransaction<'tx>: Send + AsReadable,
{
    pub fn new(store: TStore, wallet: Wallet, notifications: broadcast::Receiver<PaymentProcessorEvent>) -> Self {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        Self {
            store,
            wallet,
            notifications,
            interval,
            work_queue: futures_bounded::FuturesMap::new(
                // Tasks should implement timeouts themselves based on their expected duration, so the blanket timeout is not useful here.
                || futures_bounded::Delay::tokio(Duration::MAX),
                MAX_CONCURRENT_JOBS,
            ),
            timers: HashMap::new(),
        }
    }

    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(self.run())
    }

    pub async fn run(mut self) {
        loop {
            match self.main_task().await {
                Ok(ControlFlow::Continue(())) => {
                    // Continue looping
                },
                Ok(ControlFlow::Break(())) => {
                    log::info!("👷 Worker is stopping.");
                    break;
                },
                Err(err) => {
                    log::error!("👷 Worker encountered an error: {}", err);
                },
            }
        }
    }

    async fn main_task(&mut self) -> anyhow::Result<ControlFlow<()>> {
        tokio::select! {
            _ = self.interval.tick() => {
                self.log_queue_stats().await?;
                self.check_queue_for_more_jobs().await?;
                Ok(ControlFlow::Continue(()))
            },

            event = self.notifications.recv() => {
                match event {
                    Ok(event) => {
                        match event {
                            PaymentProcessorEvent::TaskCreated { task_id } => {
                                log::info!("👷 Worker received TaskCreated event for task_id: {}", task_id);
                                self.check_queue_for_more_jobs().await?;
                                Ok(ControlFlow::Continue(()))
                            },
                        }
                    },
                    Err(_) => {
                        // All senders have been dropped, time to shut down
                        self.shutdown().await;
                        Ok(ControlFlow::Break(()))
                    },
                }
            }

            result = poll_fn(|cx| self.work_queue.poll_unpin(cx)) => {
                let (task_id, result) = result;
                self.handle_completed_task(task_id, result).await?;
                self.interval.reset();
                self.check_queue_for_more_jobs().await?;
                Ok(ControlFlow::Continue(()))
            }
        }
    }

    async fn shutdown(&mut self) {
        log::info!("👷 Worker is shutting down. Waiting for running tasks to complete...");
        while !self.work_queue.is_empty() {
            let (job_id, result) = poll_fn(|cx| self.work_queue.poll_unpin(cx)).await;
            if let Err(err) = self.handle_completed_task(job_id, result).await {
                log::error!("👷 Error handling completed task {} during shutdown: {}", job_id, err);
            }
        }
        log::info!("👷 All running tasks have completed. Worker shutdown complete.");
    }

    fn is_work_queue_full(&self) -> bool {
        self.work_queue.len() >= MAX_CONCURRENT_JOBS
    }

    async fn log_queue_stats(&mut self) -> anyhow::Result<()> {
        let mut tx = self.store.create_read_tx().await?;
        let pending_jobs = tx.count_jobs_with_status(JobStatus::Pending).await?;
        let in_progress_jobs = tx.count_jobs_with_status(JobStatus::InProgress).await?;
        let waiting_for_balance_jobs = tx.count_jobs_with_status(JobStatus::WaitingForBalance).await?;

        let metrics = tokio::runtime::Handle::current().metrics();
        info!(
            "👷 Queue Stats - Pending: {}, In Progress: {}, Waiting for Balance: {}, Active Workers: {}/{}, Tokio: {}/{}/{}",
            pending_jobs,
            in_progress_jobs,
            waiting_for_balance_jobs,
            self.work_queue.len(),
            MAX_CONCURRENT_JOBS,
            metrics.num_alive_tasks(),
            metrics.num_workers(),
            metrics.global_queue_depth(),
        );

        Ok(())
    }

    async fn dispatch_job(&mut self, job_id: uuid::Uuid) -> anyhow::Result<bool> {
        let job = {
            let mut tx = self.store.create_read_tx().await?;
            let job = tx
                .get_job_by_id(job_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Task with ID {} not found", job_id))?;

            if !job.is_valid_transition(JobStatus::InProgress) {
                warn!(
                    "👷 Invalid status transition for job {}: {:?} -> InProgress",
                    job.id, job.status
                );
                return Ok(true);
            }
            job
        };

        info!(
            "👷 Dispatching job: {} {} - queue: {}/{}",
            job.id,
            job.job_type,
            self.work_queue.len(),
            MAX_CONCURRENT_JOBS
        );

        // Implement job dispatching logic here
        match job.job_type {
            JobType::ProcessPayment => {
                let job_id = job.id;
                let data = match serde_json::from_value(job.data.clone()) {
                    Ok(data) => data,
                    Err(err) => {
                        log::error!("👷 Failed to deserialize job data for job {}: {}", job_id, err);
                        set_job_status(&self.store, &job_id, JobStatus::Invalid).await?;
                        return Ok(true); // Skip this job
                    },
                };

                self.timers.insert(job_id, Instant::now());
                match self
                    .work_queue
                    .try_push(job_id, jobs::send_payment::do_work(self.create_context(), job, data))
                {
                    Ok(_) => {
                        set_job_status(&self.store, &job_id, JobStatus::InProgress).await?;
                        log::info!("👷 Dispatched ProcessPayment job with ID: {}", job_id);
                        Ok(true)
                    },
                    Err(futures_bounded::PushError::Replaced(_)) => {
                        warn!("👷 Already processing job, this shouldn't happen {}", job_id);
                        set_job_status(&self.store, &job_id, JobStatus::InProgress).await?;
                        Ok(true)
                    },
                    Err(futures_bounded::PushError::BeyondCapacity(_)) => {
                        log::warn!("👷 Working on maximum jobs: defer job {}", job_id);
                        Ok(false)
                    },
                }
            },
        }
    }

    fn create_context(&self) -> JobContext {
        JobContext {
            wallet: self.wallet.clone(),
        }
    }

    async fn handle_completed_task(
        &mut self,
        task_id: uuid::Uuid,
        result: Result<anyhow::Result<JobResult>, futures_bounded::Timeout>,
    ) -> anyhow::Result<()> {
        let exec_time = self.timers.remove(&task_id).map(|t| t.elapsed()).unwrap_or_default();
        let mut tx = self.store.create_write_tx().await?;
        match result {
            Ok(Ok(JobResult::Completed { result })) => {
                log::info!(
                    "👷 Task {} completed successfully ({}/{})",
                    task_id,
                    self.work_queue.len(),
                    MAX_CONCURRENT_JOBS
                );
                tx.job_set_completed_result(&task_id, exec_time, result).await?;
            },
            Ok(Ok(JobResult::RetryIn { duration })) => {
                let attempts = tx.job_requeue_for_later(&task_id, duration).await?;
                log::info!(
                    "👷 Task {} will be retried (attempt #{}) - ({}/{})",
                    task_id,
                    attempts,
                    self.work_queue.len(),
                    MAX_CONCURRENT_JOBS
                );
            },
            Ok(Ok(JobResult::WaitForBalance { resource, amount })) => {
                log::info!(
                    "👷 Task {} waiting for balance {amount} ({}/{})",
                    task_id,
                    self.work_queue.len(),
                    MAX_CONCURRENT_JOBS
                );
                tx.job_requeue_for_balance(&task_id, resource, amount).await?;
            },
            Ok(Err(err)) => {
                log::error!(
                    "👷 Task {} failed with error: {} ({}/{})",
                    task_id,
                    err,
                    self.work_queue.len(),
                    MAX_CONCURRENT_JOBS
                );
                tx.set_failure_reason(&task_id, err.to_string()).await?;
            },
            Err(_) => {
                log::error!(
                    "👷 Task {} timed out ({}/{})",
                    task_id,
                    self.work_queue.len(),
                    MAX_CONCURRENT_JOBS
                );
                tx.job_set_status(&task_id, JobStatus::TimedOut).await?;
            },
        }

        tx.commit().await?;

        Ok(())
    }

    async fn check_queue_for_more_jobs(&mut self) -> anyhow::Result<()> {
        if self.is_work_queue_full() {
            log::info!(
                "👷 Work queue is full ({}), not checking for more jobs",
                MAX_CONCURRENT_JOBS
            );
            return Ok(());
        }

        let account = self.wallet.get_account_or_default(None)?;
        let balances = self.wallet.get_balances(account.component_address())?;
        loop {
            let job_id = {
                let mut tx = self.store.create_read_tx().await?;
                let mut unpaused_job = None;
                for balance in &balances {
                    if let Some(id) = tx
                        .get_next_job_waiting_for_balance(
                            balance.resource_address,
                            balance.total_balance().to_u64_checked().unwrap_or(u64::MAX),
                        )
                        .await?
                    {
                        info!("👷 Found unpaused job waiting for balance: {}", id);
                        unpaused_job = Some(id);
                        break;
                    }
                }
                match unpaused_job {
                    Some(id) => Some(id),
                    None => tx.get_next_job_id().await?,
                }
            };

            let Some(job_id) = job_id else {
                break;
            };
            log::info!("👷 Found job in queue: {}", job_id);
            if !self.dispatch_job(job_id).await? {
                log::info!(
                    "👷 Deferring job dispatch due to max capacity ({})",
                    MAX_CONCURRENT_JOBS
                );
                break;
            }
        }
        Ok(())
    }
}

async fn set_job_status<TStore: WriteableStore>(
    store: &TStore,
    id: &uuid::Uuid,
    status: JobStatus,
) -> anyhow::Result<()> {
    let mut tx = store.create_write_tx().await?;
    tx.job_set_status(id, status).await?;
    tx.commit().await?;
    Ok(())
}
