use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use reqwest::Client;
use tokio::task::JoinSet;
use tracing::{error, info};

use crate::core::{
    feed::feedparser,
    library::{data::librarydata::LibraryData, feedcategory::FeedCategory, feeditem::FeedItem},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CONCURRENT_FETCHES: usize = 16;

#[derive(Debug)]
pub enum FeedUpdateStatus {
    Updated,
    Skipped,
    Failed(String),
}

impl FeedUpdateStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Updated => "updated",
            Self::Skipped => "skipped",
            Self::Failed(_) => "failed",
        }
    }
}

#[derive(Debug, Default)]
pub struct FeedUpdateSummary {
    pub total: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
}

pub async fn update_feeds(
    feedcategories: Vec<FeedCategory>,
    data_dir: &Path,
    parallel_feed_updates: Option<bool>,
    mut on_complete: impl FnMut(&str, &FeedUpdateStatus),
) -> color_eyre::Result<FeedUpdateSummary> {
    let feeds: Vec<FeedItem> = feedcategories
        .into_iter()
        .flat_map(|category| category.feeds)
        .collect();
    let mut summary = FeedUpdateSummary {
        total: feeds.len(),
        ..FeedUpdateSummary::default()
    };
    let mut pending = VecDeque::new();

    for feed in feeds {
        if LibraryData::feed_needs_update(&feed) {
            pending.push_back(feed);
        } else {
            summary.skipped += 1;
            on_complete(&feed.title, &FeedUpdateStatus::Skipped);
        }
    }

    if pending.is_empty() {
        return Ok(summary);
    }

    let parallelism = if matches!(parallel_feed_updates, Some(true)) {
        MAX_CONCURRENT_FETCHES.min(pending.len())
    } else {
        1
    };

    let client = Client::builder()
        .user_agent(format!("bulletty/{}", env!("CARGO_PKG_VERSION")))
        .timeout(REQUEST_TIMEOUT)
        .build()?;
    let data = LibraryData::new(data_dir);
    let mut tasks = JoinSet::new();
    let fetch = |feed: FeedItem| {
        let client = client.clone();
        async move {
            let result = feedparser::get_feed_entries(&client, &feed).await;
            (feed, result)
        }
    };

    // seed `parallelism` tasks
    for feed in pending.drain(..parallelism) {
        tasks.spawn(fetch(feed));
    }

    while let Some(task_result) = tasks.join_next().await {
        match task_result {
            Ok((feed, Ok(entries))) => {
                match data.apply_feed_update(&feed.category, &feed, entries) {
                    Ok(()) => {
                        summary.updated += 1;
                        on_complete(&feed.title, &FeedUpdateStatus::Updated);
                    }
                    Err(update_error) => {
                        summary.failed += 1;
                        on_complete(
                            &feed.title,
                            &FeedUpdateStatus::Failed(update_error.to_string()),
                        );
                    }
                }
            }
            Ok((feed, Err(fetch_error))) => {
                summary.failed += 1;
                on_complete(
                    &feed.title,
                    &FeedUpdateStatus::Failed(fetch_error.to_string()),
                );
            }
            Err(task_error) => {
                summary.failed += 1;
                on_complete(
                    "unknown feed",
                    &FeedUpdateStatus::Failed(format!("Feed update task failed: {task_error}")),
                );
            }
        }

        // start a new task since one has finished
        if let Some(feed) = pending.pop_front() {
            tasks.spawn(fetch(feed));
        }
    }

    Ok(summary)
}

pub struct Updater {
    pub last_completed: Arc<Mutex<String>>,
    pub total_completed: Arc<AtomicUsize>,
    pub total_updated: Arc<AtomicUsize>,
    pub finished: Arc<AtomicBool>,
    pub total: usize,

    _thread: Option<JoinHandle<()>>,
}

impl Updater {
    pub fn new(
        feedcategories: Vec<FeedCategory>,
        data_dir: &Path,
        parallel_feed_updates: Option<bool>,
    ) -> Self {
        let total = feedcategories
            .iter()
            .map(|category| category.feeds.len())
            .sum();
        let last_completed = Arc::new(Mutex::new(String::from("Working...")));
        let finished = Arc::new(AtomicBool::new(false));
        let total_completed = Arc::new(AtomicUsize::new(0));
        let total_updated = Arc::new(AtomicUsize::new(0));

        let last_completed_clone = Arc::clone(&last_completed);
        let finished_clone = Arc::clone(&finished);
        let total_completed_clone = Arc::clone(&total_completed);
        let total_updated_clone = Arc::clone(&total_updated);
        let data_dir: PathBuf = data_dir.into();

        let handle = Some(thread::spawn(move || {
            info!("Starting updater");
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(runtime_error) => {
                    error!("Failed to start feed updater runtime: {runtime_error}");
                    finished_clone.store(true, Ordering::Release);
                    return;
                }
            };

            let result = runtime.block_on(update_feeds(
                feedcategories,
                &data_dir,
                parallel_feed_updates,
                |title, status| {
                    let completed = total_completed_clone.fetch_add(1, Ordering::Relaxed) + 1;
                    if matches!(status, FeedUpdateStatus::Updated) {
                        total_updated_clone.fetch_add(1, Ordering::Relaxed);
                    }
                    *last_completed_clone.lock().unwrap() =
                        format!("{completed}/{total}: {} ({})", title, status.label());

                    if let FeedUpdateStatus::Failed(update_error) = status {
                        error!("Failed to update {title}: {update_error}");
                    } else {
                        info!("{} {title}", status.label());
                    }
                },
            ));

            if let Err(update_error) = result {
                error!("Feed update batch failed: {update_error}");
            }
            finished_clone.store(true, Ordering::Release);
        }));

        Self {
            last_completed,
            total_completed,
            total_updated,
            finished,
            total,
            _thread: handle,
        }
    }
}
