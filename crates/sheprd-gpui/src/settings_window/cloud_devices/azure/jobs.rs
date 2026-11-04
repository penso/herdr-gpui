//! Azure machines being created, in the Azure devices box: the main window
//! owns the jobs (see `crate::cloud::jobs`), and Settings shows each one's
//! latest step with a progress bar until it is saved and listed below.

use super::super::super::SettingsWindow;
use crate::{
    cloud::CloudProvider,
    progress::{self, Progress},
};
use gpui::{prelude::*, *};

/// A job being drawn, read from the main window.
struct Creating {
    name: String,
    status: String,
    progress: Option<f32>,
}

impl SettingsWindow {
    fn azure_jobs(&self, cx: &App) -> Vec<Creating> {
        let Some(source) = self.source.upgrade() else {
            return Vec::new();
        };
        source
            .read(cx)
            .cloud_jobs
            .iter()
            .filter(|job| job.provider == CloudProvider::Azure)
            .map(|job| Creating {
                name: job.name.clone(),
                status: job.status.clone(),
                progress: job.progress,
            })
            .collect()
    }

    /// One row per Azure machine being created, above the saved devices.
    pub(super) fn azure_job_rows(&self, cx: &App) -> Vec<Div> {
        self.azure_jobs(cx)
            .into_iter()
            .enumerate()
            .map(|(index, job)| {
                let progress = job.progress.map_or(Progress::Busy, Progress::Working);
                div()
                    .debug_selector(move || format!("cloud-azure-creating-{index}"))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .min_w_0()
                    .child(div().truncate().child(job.name))
                    .child(progress::bar(
                        "cloud-azure-create-progress",
                        progress,
                        crate::menu::accent(&self.theme).into(),
                        rgb(self.theme.active).into(),
                    ))
                    .child(
                        div()
                            .truncate()
                            .text_color(rgb(self.theme.muted))
                            .child(job.status),
                    )
            })
            .collect()
    }
}
