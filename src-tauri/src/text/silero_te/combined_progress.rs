#![cfg(feature = "silero-te")]

use tauri::{AppHandle, Emitter};

use crate::app::events::SILERO_TE_DOWNLOAD_PROGRESS;
use crate::transcription::model_store::DownloadProgress;

/// Rough download sizes for a single 0–100% bar (runtime zip + TE weights).
pub const RUNTIME_BUDGET_BYTES: u64 = 250_000_000;
pub const ASSETS_BUDGET_BYTES: u64 = 95_000_000;

pub struct CombinedSileroTeProgress {
    app: AppHandle,
    runtime_done: u64,
    assets_done: u64,
    runtime_budget: u64,
}

impl CombinedSileroTeProgress {
    pub fn new(app: AppHandle) -> Self {
        let mut reporter = Self {
            app,
            runtime_done: 0,
            assets_done: 0,
            runtime_budget: RUNTIME_BUDGET_BYTES,
        };
        reporter.emit();
        reporter
    }

    fn total_budget(&self) -> u64 {
        self.runtime_budget.saturating_add(ASSETS_BUDGET_BYTES)
    }

    fn emit(&self) {
        let downloaded = self.runtime_done.saturating_add(self.assets_done);
        let _ = self.app.emit(
            SILERO_TE_DOWNLOAD_PROGRESS,
            DownloadProgress::new(downloaded, Some(self.total_budget())),
        );
    }

    pub fn runtime_zip(&mut self, progress: DownloadProgress) {
        if let Some(total) = progress.total.filter(|value| *value > 0) {
            self.runtime_budget = total.max(RUNTIME_BUDGET_BYTES / 2);
        }
        self.runtime_done = progress.downloaded.min(self.runtime_budget);
        self.emit();
    }

    pub fn runtime_installing(&mut self) {
        self.runtime_done = self.runtime_budget.saturating_mul(95).saturating_div(100);
        self.emit();
    }

    pub fn runtime_complete(&mut self) {
        self.runtime_done = self.runtime_budget;
        self.emit();
    }

    pub fn assets(&mut self, progress: DownloadProgress) {
        self.assets_done = progress.downloaded.min(ASSETS_BUDGET_BYTES);
        self.emit();
    }

    pub fn complete(&mut self) {
        self.runtime_done = self.runtime_budget;
        self.assets_done = ASSETS_BUDGET_BYTES;
        self.emit();
    }
}
