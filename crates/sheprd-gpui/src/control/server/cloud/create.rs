//! `cloud create`: checks what was asked for, then asks in the window. The
//! user confirms there; nothing is created from here.
use super::{MAX_DEVICES, Registry, SheprdWindow, answer, devices, invalid};
use crate::{
    cloud::{self, CloudProvider, Shape, Spec, batch::create::Creation},
    control::{
        batch::{CreateOp, MAX_CREATE},
        protocol::{ErrorCode, Response},
    },
};
use gpui::{Context, Window};

pub(super) fn ask(
    view: &mut SheprdWindow,
    op: CreateOp,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
) -> Response {
    let registry = cx.default_global::<Registry>().clone();
    let creation = match creation(view, op, &registry) {
        Ok(creation) => creation,
        Err(response) => return response,
    };
    let Some(id) = view.ask_to_create_cloud(creation, window, cx) else {
        return Response::error(
            ErrorCode::WindowBusy,
            "A dialog is open in the window; close it first",
        );
    };
    crate::control::server::work::bring_forward(window, cx);
    match registry.get(id) {
        Some(batch) => answer(batch),
        None => Response::error(ErrorCode::Failed, "The batch was not kept"),
    }
}

/// The provider a request names by key or name.
fn provider(text: &str) -> Result<CloudProvider, Response> {
    CloudProvider::ALL
        .iter()
        .copied()
        .find(|provider| {
            provider.key().eq_ignore_ascii_case(text)
                || cloud::name(*provider).eq_ignore_ascii_case(text)
        })
        .ok_or_else(|| {
            let known: Vec<&str> = CloudProvider::ALL.iter().map(|p| p.key()).collect();
            Response::error(
                ErrorCode::NotFound,
                format!(
                    "No cloud provider is named {text}; this build has {}",
                    known.join(", ")
                ),
            )
        })
}

/// What `op` asks for, checked: a provider that can create machines here,
/// options that apply to it, and room in the device list.
pub(super) fn creation(
    view: &SheprdWindow,
    op: CreateOp,
    registry: &Registry,
) -> Result<Creation, Response> {
    let provider = provider(&op.provider)?;
    let name = cloud::name(provider);
    if let Some(reason) = cloud::unavailable() {
        return Err(Response::error(ErrorCode::Unsupported, reason));
    }
    if cloud::provision(provider).is_none() {
        return Err(Response::error(
            ErrorCode::Unsupported,
            format!("{name} machines cannot be created from the command line"),
        ));
    }
    if !(1..=MAX_CREATE).contains(&op.count) {
        return Err(invalid(format!(
            "Create between 1 and {MAX_CREATE} machines at once"
        )));
    }
    #[cfg(feature = "coder")]
    let coder = provider == CloudProvider::Coder;
    #[cfg(not(feature = "coder"))]
    let coder = false;
    if !coder && (op.template.is_some() || op.preset.is_some()) {
        return Err(invalid(format!(
            "{name} has no templates or presets; only Coder does"
        )));
    }
    #[cfg(feature = "azure")]
    let azure = provider == CloudProvider::Azure;
    #[cfg(not(feature = "azure"))]
    let azure = false;
    if !azure && op.shape == Shape::Sandbox {
        return Err(invalid(format!(
            "{name} makes one kind of machine; --sandbox is for Azure"
        )));
    }
    let listed = devices(view);
    let saved = listed.len()
        + registry.creating(|endpoint| {
            listed
                .iter()
                .any(|device| device.target.endpoint == endpoint)
        });
    if saved + op.count as usize > MAX_DEVICES {
        return Err(invalid(format!(
            "At most {MAX_DEVICES} cloud devices are kept, and {saved} are saved or being made"
        )));
    }
    Ok(Creation {
        provider,
        count: op.count,
        spec: Spec {
            template: op.template,
            preset: op.preset,
            shape: op.shape,
            install: op.install,
            ..Spec::new()
        },
        setup: op.setup,
    })
}

#[cfg(test)]
mod tests;
