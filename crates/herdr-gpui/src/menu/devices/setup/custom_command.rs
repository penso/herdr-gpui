//! A custom transport cannot be passed to upstream's `machine add`. Save only
//! its standard catalog entry; the normal bridge starts the session on attach.
use super::*;

pub(super) fn save(request: &Request, claim: &Claim) -> Result<()> {
    ensure_unsaved(request, claim, &load_catalog()?, |target| {
        request.resolve(target)
    })?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    // Publish the executable preference before the catalog can expose this host.
    crate::config::Config::save_device_ssh_command(&id, &request.target.command)?;
    herdr_client::store_saved_host(
        false,
        SavedHost {
            id: id.clone(),
            label: request.label.clone(),
            target: request.target.target.clone(),
            session: request.session.clone(),
            enabled: true,
        },
    )?;
    // Upstream's CLI does not lock its catalog. Apply the same oldest-profile
    // rule as ordinary setup if another client saved an alias in between.
    let hosts = load_catalog()?;
    if let Some(first) = first_saved(request, claim, &hosts, |target| request.resolve(target))
        && first.id != id
    {
        remove(&id)?;
        crate::config::Config::save_device_ssh_command(&id, &SshCommand::default())?;
        return Err(Error::DeviceExists(first.label.clone()));
    }
    Ok(())
}
