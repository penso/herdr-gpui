//! Machines being added, owned by the window rather than a dialog: a build can
//! take minutes, so the dialog closes at once and each job runs on its own
//! worker. The device picker lists them with their latest status; a toast
//! reports each one ready (or setup failed). Closing the window drops the jobs,
//! which cancels them. Nothing here knows a provider's API: a job is the
//! provider's own work, reporting the shared [`Step`]s.

use super::worker::{self, Worker};
use crate::HerdrWindow;
use crate::cloud::{CloudProvider, SavedDevice, Step};
use gpui::Context;

enum Update {
    Step(Step),
    Finished(crate::Result<SavedDevice>),
}

pub(crate) struct Job {
    id: u64,
    pub(crate) provider: CloudProvider,
    pub(crate) name: String,
    pub(crate) status: String,
    /// How far along the job is, from its latest step; `None` before one.
    pub(crate) progress: Option<f32>,
    /// The setup script the device runs once shown, fixed when it was asked
    /// for so an edit to the config meanwhile cannot change it.
    setup: Option<String>,
    _worker: Worker,
}

#[derive(Default)]
pub(crate) struct Jobs {
    jobs: Vec<Job>,
    next: u64,
    /// How many jobs have finished, so a view showing saved devices (such as
    /// Settings) can tell when to read them again.
    finished: u64,
    /// Bumped whenever a job starts, steps, or finishes, so a view showing
    /// their progress redraws for that and not for the window's other changes.
    revision: u64,
}

impl Jobs {
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Job> {
        self.jobs.iter()
    }

    pub(crate) fn len(&self) -> usize {
        self.jobs.len()
    }

    pub(crate) fn finished(&self) -> u64 {
        self.finished
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Count a job as finished without running one.
    #[cfg(all(test, any(feature = "coder", feature = "daytona")))]
    pub(crate) fn finish_for_test(&mut self) {
        self.finished += 1;
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Whether any job adds a machine of this provider.
    #[cfg(feature = "railway")]
    pub(crate) fn has(&self, provider: CloudProvider) -> bool {
        self.jobs.iter().any(|job| job.provider == provider)
    }

    /// Whether a job already adds a machine of this provider with this name.
    #[cfg(feature = "coder")]
    pub(crate) fn contains(&self, provider: CloudProvider, name: &str) -> bool {
        self.jobs
            .iter()
            .any(|job| job.provider == provider && job.name == name)
    }
}

impl HerdrWindow {
    /// Run `work` to add the machine `name` from `provider`; it reports
    /// through the picker and a toast. `work` blocks on its own worker. A
    /// machine added with a `setup` script runs it once a window shows it.
    pub(crate) fn start_cloud_job<E: Into<crate::Error>>(
        &mut self,
        provider: CloudProvider,
        name: String,
        setup: Option<String>,
        work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(Step)) -> Result<SavedDevice, E> + Send + 'static,
        cx: &mut Context<Self>,
    ) -> crate::Result<()> {
        if let Some(reason) = crate::cloud::unavailable() {
            return Err(crate::cloud::Error::Unavailable(reason).into());
        }
        let id = self.cloud_jobs.next;
        self.cloud_jobs.next += 1;
        let worker = worker::spawn(
            "herdr-cloud-job",
            cx,
            move |cancelled, send| {
                let result = work(cancelled, &|step| send(Update::Step(step))).map_err(Into::into);
                send(Update::Finished(result));
            },
            move |this: &mut Self, update, cx| this.apply_cloud_job(id, update, cx),
        )
        .map_err(|error| {
            tracing::error!(category = "cloud_worker", error_kind = ?error.kind(), "Could not start cloud job worker");
            crate::cloud::Error::Worker("job")
        })?;
        self.cloud_jobs.jobs.push(Job {
            id,
            provider,
            name,
            status: crate::cloud::status::contacting(provider),
            progress: None,
            setup,
            _worker: worker,
        });
        self.cloud_jobs.revision += 1;
        cx.notify();
        Ok(())
    }

    fn apply_cloud_job(&mut self, id: u64, update: Update, cx: &mut Context<Self>) {
        let Some(index) = self.cloud_jobs.jobs.iter().position(|job| job.id == id) else {
            return;
        };
        self.cloud_jobs.revision += 1;
        match update {
            Update::Step(step) => {
                let job = &mut self.cloud_jobs.jobs[index];
                job.status = crate::cloud::status::step(&step);
                job.progress = Some(step.progress());
            }
            Update::Finished(result) => {
                let job = self.cloud_jobs.jobs.remove(index);
                self.cloud_jobs.finished += 1;
                let (name, noun) = (
                    crate::cloud::name(job.provider),
                    crate::cloud::noun(job.provider),
                );
                match result {
                    Ok(saved) => {
                        let then = if job.setup.is_some() {
                            "; its setup script runs in a new tab when it connects"
                        } else {
                            " to start working"
                        };
                        self.local_transfer_notice(
                            &format!("{name} {noun} ready"),
                            format!(
                                "{} is ready. Choose it in the device picker{then}.",
                                saved.label
                            ),
                            cx,
                        );
                        if let Some(script) = job.setup {
                            let pending = crate::cloud_setup::Pending::new(
                                saved.endpoint_id(),
                                saved.label,
                                job.provider,
                                script,
                                std::time::Instant::now(),
                            );
                            self.queue_cloud_setup(pending, cx);
                        }
                    }
                    Err(error) => self.local_transfer_notice(
                        &format!("{name} {noun} setup failed"),
                        format!("{}: {error}", job.name),
                        cx,
                    ),
                }
            }
        }
        cx.notify();
    }
}
