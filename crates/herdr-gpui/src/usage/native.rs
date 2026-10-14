//! Synthetic plan readings for the isolated native Settings smoke check.
use super::*;
use anyhow::{Context as _, Result};

pub(crate) fn fixture() -> Result<Usage> {
    let providers = [
        registry::find("codex").context("native Codex provider missing")?,
        registry::find("claude").context("native Claude provider missing")?,
    ];
    let readings = providers
        .into_iter()
        .map(|provider| Reading {
            provider,
            report: Some(report(provider)),
            error: None,
            access: None,
        })
        .collect();
    let (requests, incoming) = mpsc::sync_channel::<Job>(1);
    let (outgoing, results) = mpsc::sync_channel(4);
    thread::Builder::new()
        .name("herdr-native-usage-fixture".into())
        .spawn(move || {
            for job in incoming {
                for provider in providers {
                    if job.config.hidden(provider) {
                        continue;
                    }
                    if outgoing
                        .send(Message::Reading(
                            job.host.clone(),
                            provider,
                            Ok(report(provider)),
                        ))
                        .is_err()
                    {
                        return;
                    }
                }
                if outgoing.send(Message::Done(job.host, Ok(()))).is_err() {
                    return;
                }
            }
        })
        .context("native usage fixture worker")?;
    Ok(Usage {
        host: Some(Host::Local),
        entries: HashMap::from([(
            Host::Local,
            Entry {
                readings,
                due: Some(Instant::now() + REFRESH),
                ..Default::default()
            },
        )]),
        worker: Some(Worker { requests, results }),
        ..Default::default()
    })
}

fn report(provider: Provider) -> Report {
    Report::new(
        provider,
        model::Account::default(),
        vec![model::Window::new(model::Kind::Session, 35., None, None)],
    )
}

impl Usage {
    pub(crate) fn native_forget_claude_report(&mut self) -> Result<()> {
        let reading = self
            .entries
            .get_mut(&Host::Local)
            .and_then(|entry| {
                entry
                    .readings
                    .iter_mut()
                    .find(|reading| reading.provider.id() == "claude")
            })
            .context("missing native Claude reading")?;
        reading.report = None;
        reading.access = Some(Access::Needed);
        Ok(())
    }
}
