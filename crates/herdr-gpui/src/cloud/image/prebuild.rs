//! Building a project's image ahead of a dispatch, from a workspace's menu or
//! Settings. The main window owns these builds, so closing Settings does not
//! cancel one; the shared [`crate::cloud::image::builds`] registry shows their progress
//! wherever it is drawn, and a notice reports how each ended.

use crate::cloud::image::{Checkout, Error, Unsupported, builds};
use crate::{
    HerdrWindow,
    cloud::{
        CloudProvider,
        worker::{self, Worker},
    },
};
use gpui::Context;

/// The builds a window started, until each ends.
#[derive(Default)]
pub(crate) struct Prebuilds {
    next: u64,
    running: Vec<(u64, Worker)>,
}

/// How a prebuild ended, for the notice.
enum Ended {
    Built,
    NoRecipe,
    Failed(String),
}

impl HerdrWindow {
    /// Build `provider`'s image for the project `label` checked out at `path`
    /// on the endpoint `endpoint_id`, unless one built from the same recipe
    /// is there already. Fails at once when the endpoint cannot be scripted.
    pub(crate) fn build_sandbox_image(
        &mut self,
        provider: CloudProvider,
        endpoint_id: &str,
        path: String,
        label: String,
        cx: &mut Context<Self>,
    ) -> crate::Result<()> {
        let support = crate::cloud::image_support(provider)
            .ok_or(Error::Unsupported(Unsupported::Provider))?;
        let endpoint = self
            .endpoints
            .iter()
            .find(|endpoint| endpoint.id == endpoint_id)
            .ok_or(Error::Unknown)?;
        let host = crate::teleport::host_for(&endpoint.connection.target)?;
        let endpoint_id = endpoint_id.to_owned();
        let id = self.sandbox_image_builds.next;
        self.sandbox_image_builds.next += 1;
        let notice_label = label.clone();
        let worker = worker::spawn_flagged(
            "herdr-sandbox-image",
            cx,
            move |cancelled, send| {
                let checkout = Checkout {
                    host: &host,
                    endpoint_id: &endpoint_id,
                    path: &path,
                    label: &label,
                };
                let stopped = || cancelled.load(std::sync::atomic::Ordering::Acquire);
                let ended = match crate::cloud::image::read(&checkout, cancelled) {
                    Ok(None) => Ended::NoRecipe,
                    Ok(Some(recipe)) => {
                        match builds::ensure(provider, support, &recipe, &stopped, &|_| {}) {
                            Ok(_) => Ended::Built,
                            Err(error) => Ended::Failed(error.to_string()),
                        }
                    }
                    Err(error) => Ended::Failed(error.to_string()),
                };
                send(ended);
            },
            move |this: &mut Self, ended, cx| {
                this.sandbox_image_builds
                    .running
                    .retain(|(running, _)| *running != id);
                let name = crate::cloud::name(provider);
                let (title, body) = match ended {
                    Ended::Built => (
                        format!("{name} image ready"),
                        format!("New {}s for {notice_label} start from it.", crate::cloud::noun(provider)),
                    ),
                    Ended::NoRecipe => (
                        format!("No {name} image to build"),
                        format!(
                            "{notice_label} has no .herdr/sandbox/Dockerfile or dev container to build from."
                        ),
                    ),
                    Ended::Failed(error) => (
                        format!("{name} image build failed"),
                        format!("{notice_label}: {error}"),
                    ),
                };
                this.local_transfer_notice(&title, body, cx);
            },
        )
        .map_err(|_| crate::cloud::Error::Worker(crate::cloud::Worker::ImageBuild))?;
        self.sandbox_image_builds.running.push((id, worker));
        Ok(())
    }
}
