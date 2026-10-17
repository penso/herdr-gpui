//! Azure Container Apps sandboxes as Herdr devices. A sandbox group is an ARM
//! resource; the sandboxes in it live on the group's regional data plane
//! ("ADC", `management.<region>.azuredevcompute.io`), which takes its own
//! token audience. Sandboxes run no SSH server: the only way in is the data
//! plane's exec stream, which [`relay`] turns into the standard streams
//! Herdr's bridge expects.

mod access;
mod api;
pub(crate) mod relay;
mod setup;

pub(crate) use setup::{add_device, command, delete, exists, running_command, sizes};

#[cfg(test)]
mod tests;
