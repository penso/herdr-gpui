//! Every provider described once. A provider is one row of the table at the
//! bottom: its feature, module, words, logo, how its command is prepared,
//! whether removing a device can delete the machine there too, which sandbox
//! images it holds, and how it makes a new machine without a dialog. The macro
//! turns the rows into the `match`es the rest of the app calls, so supporting
//! another provider adds a row here rather than an arm in each of them.

use super::{CloudProvider, Error};
use herdr_client::{ConnectTarget, Transport};
pub use herdr_control_protocol::protocol::cloud::Shape;
use std::{io, process::Command, sync::atomic::AtomicBool};

/// Deleting a saved device's machine at its provider, which Remove offers as
/// its default answer for the providers that have it.
#[derive(Clone, Copy, Debug)]
pub struct Deletion {
    /// What the machine `id` is called, such as `VM` or `sandbox`.
    pub noun: fn(&str) -> &'static str,
    /// What goes with the machine, as the confirmation says it, such as
    /// "with its disk and IP".
    pub leftovers: &'static str,
    /// Delete the saved device `id` at the provider and forget it here. Blocks:
    /// run it on a worker. A device no longer saved has nothing left to do.
    pub run: fn(&str) -> crate::Result<()>,
}

/// What a new machine should be, as `cloud create` or dispatch asks for it.
/// A provider reads what applies to it and ignores the rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    /// Its name; `None` lets the provider pick one.
    pub name: Option<String>,
    /// A sandbox image to start from, for a provider that holds them.
    pub image: Option<String>,
    /// Whether Herdr's installer may run on a machine that lacks Herdr.
    pub install: bool,
    /// Coder: the template, by name or ID; `None` takes the only one.
    pub template: Option<String>,
    /// Coder: the template's preset, by name; `None` takes its default.
    pub preset: Option<String>,
    /// Azure: a VM or a Container Apps sandbox.
    pub shape: Shape,
}

impl Default for Spec {
    fn default() -> Self {
        Self::new()
    }
}

impl Spec {
    /// The provider's usual machine, named by the provider, with Herdr
    /// installed when it lacks it.
    pub fn new() -> Self {
        Self {
            name: None,
            image: None,
            install: true,
            template: None,
            preset: None,
            shape: Shape::Machine,
        }
    }
}

/// Make a new, ready machine as `spec` says and save it as a device,
/// reporting each step. Blocks: run it on a worker.
pub type Provision =
    fn(&Spec, &dyn Fn() -> bool, &dyn Fn(super::Step)) -> crate::Result<super::SavedDevice>;

macro_rules! providers {
    ($(
        $feature:literal $variant:ident => {
            module: $module:ident,
            name: $name:literal,
            icon: $icon:literal,
            offer: $offer:literal,
            noun: $noun:literal,
            command: $command:ident,
            deletion: $deletion:expr,
            image: $image:expr,
            provision: $provision:expr $(,)?
        }
    ),* $(,)?) => {
        /// A provider's own failure, as one variant of the app's root error,
        /// so supporting a provider never edits that enum.
        #[derive(Debug, thiserror::Error)]
        pub enum ProviderError {
            $(#[cfg(feature = $feature)]
            #[error(transparent)]
            $variant(#[from] crate::$module::Error),)*
        }

        $(#[cfg(feature = $feature)]
        impl From<crate::$module::Error> for crate::Error {
            fn from(error: crate::$module::Error) -> Self {
                Self::Provider(ProviderError::$variant(error))
            }
        })*

        /// The provider's name, as the UI shows it.
        pub fn name(provider: CloudProvider) -> &'static str {
            match provider {
                $(#[cfg(feature = $feature)] CloudProvider::$variant => $name,)*
            }
        }

        /// The provider's logo, an embedded asset path.
        pub fn icon(provider: CloudProvider) -> &'static str {
            match provider {
                $(#[cfg(feature = $feature)] CloudProvider::$variant => $icon,)*
            }
        }

        /// What adding a device from the provider does, as Add Device says it.
        pub fn offer(provider: CloudProvider) -> &'static str {
            match provider {
                $(#[cfg(feature = $feature)] CloudProvider::$variant => $offer,)*
            }
        }

        /// What the provider calls one machine.
        pub fn noun(provider: CloudProvider) -> &'static str {
            match provider {
                $(#[cfg(feature = $feature)] CloudProvider::$variant => $noun,)*
            }
        }

        /// How removing one of the provider's devices deletes its machine, if
        /// it can.
        pub fn deletion(provider: CloudProvider) -> Option<Deletion> {
            match provider {
                $(#[cfg(feature = $feature)] CloudProvider::$variant => $deletion,)*
            }
        }

        /// How the provider holds sandbox images built from a project's
        /// recipe, if it can.
        pub fn image(provider: CloudProvider) -> Option<&'static super::image::Support> {
            match provider {
                $(#[cfg(feature = $feature)] CloudProvider::$variant => $image,)*
            }
        }

        /// How the provider makes a new machine on demand, without a
        /// dialog, if it can.
        pub fn provision(provider: CloudProvider) -> Option<Provision> {
            match provider {
                $(#[cfg(feature = $feature)] CloudProvider::$variant => $provision,)*
            }
        }

        /// The connector for `ConnectTarget::Cloud`: the provider makes the
        /// machine ready and spawns the bridge command. Runs on the connection
        /// worker.
        pub fn connect(target: &ConnectTarget, stop: &AtomicBool) -> io::Result<Transport> {
            let ConnectTarget::Cloud {
                provider,
                account,
                id,
                session,
                ..
            } = target
            else {
                return Err(io::Error::other(Error::Invalid(super::Invalid::Target)));
            };
            match provider {
                $(#[cfg(feature = $feature)]
                CloudProvider::$variant => crate::$module::connect(account, id, session, stop),)*
            }
        }

        /// Run `run` with the provider's authenticated command for the machine
        /// `id`, keeping whatever it needs alive until `run` returns.
        pub(super) fn run_command<T>(
            provider: CloudProvider,
            account: &str,
            id: &str,
            cancelled: &AtomicBool,
            run: impl FnOnce(Command) -> T,
        ) -> io::Result<T> {
            match provider {
                $(#[cfg(feature = $feature)]
                CloudProvider::$variant => {
                    providers!(@command $command, $module, account, id, cancelled, run)
                })*
            }
        }
    };
    // The provider returns the command itself.
    (@command plain, $module:ident, $account:ident, $id:ident, $cancelled:ident, $run:ident) => {{
        let command = crate::$module::command($account, $id, $cancelled).map_err(io::Error::other)?;
        Ok($run(command))
    }};
    // The provider returns a gateway that owns the command's credentials.
    (@command gateway, $module:ident, $account:ident, $id:ident, $cancelled:ident, $run:ident) => {{
        let gateway = crate::$module::command($account, $id, $cancelled).map_err(io::Error::other)?;
        Ok(gateway.with_command($run))
    }};
}

providers! {
    // keep-sorted start
    "azure" Azure => {
        module: azure,
        name: "Azure",
        icon: "icons/cloud/azure.svg",
        offer: "Create an Azure VM or sandbox in Settings",
        noun: "machine",
        command: plain,
        deletion: Some(Deletion {
            noun: |id| crate::azure::machine::Machine::parse(id).noun(),
            leftovers: ", with its disk and IP,",
            run: crate::azure::destroy::destroy_saved,
        }),
        image: None,
        provision: Some(crate::azure::create::provision),
    },
    "boat" Boat => {
        module: boat,
        name: "Boat",
        icon: "icons/terminal.svg",
        offer: "Create a Boat sandbox in Settings",
        noun: "sandbox",
        command: gateway,
        deletion: Some(Deletion {
            noun: |_| "sandbox",
            leftovers: ", with its snapshots,",
            run: crate::boat::destroy::destroy_saved,
        }),
        image: None,
        provision: Some(crate::boat::create::provision),
    },
    "coder" Coder => {
        module: coder,
        name: "Coder",
        icon: "icons/cloud/coder.svg",
        offer: "Create or attach a Coder workspace",
        noun: "workspace",
        command: plain,
        deletion: Some(Deletion {
            noun: |_| "workspace",
            leftovers: ", with its disk,",
            run: crate::coder::setup::destroy_saved,
        }),
        image: None,
        provision: Some(crate::coder::create::provision),
    },
    "container" Container => {
        module: container,
        name: "Container",
        icon: "icons/cloud/container.svg",
        offer: "Create a Docker or Podman container in Settings",
        noun: "container",
        command: plain,
        deletion: Some(Deletion {
            noun: |_| "container",
            leftovers: ", with its home volume,",
            run: crate::container::destroy_saved,
        }),
        image: Some(&crate::container::image::SUPPORT),
        provision: Some(crate::container::setup::provision),
    },
    // Sandboxes are created in Settings; existing ones cannot be attached yet.
    "daytona" Daytona => {
        module: daytona,
        name: "Daytona",
        icon: "icons/cloud/daytona.svg",
        offer: "Create a Daytona sandbox in Settings",
        noun: "sandbox",
        command: gateway,
        deletion: Some(Deletion {
            noun: |_| "sandbox",
            leftovers: "",
            run: crate::daytona::destroy::destroy_saved,
        }),
        image: Some(&crate::daytona::image::SUPPORT),
        provision: Some(crate::daytona::create::provision),
    },
    "opensandbox" OpenSandbox => {
        module: opensandbox,
        name: "OpenSandbox",
        icon: "icons/cloud/opensandbox.svg",
        offer: "Create a sandbox on an OpenSandbox server in Settings",
        noun: "sandbox",
        command: plain,
        deletion: Some(Deletion {
            noun: |_| "sandbox",
            leftovers: "",
            run: crate::opensandbox::destroy_saved,
        }),
        image: None,
        provision: Some(crate::opensandbox::setup::provision),
    },
    "railway" Railway => {
        module: railway,
        name: "Railway",
        icon: "icons/cloud/railway.svg",
        offer: "Set up a Railway VM in Settings",
        noun: "VM",
        command: plain,
        deletion: None,
        image: None,
        provision: Some(crate::railway::create::provision),
    },
    "sprites" Sprites => {
        module: sprites,
        name: "Sprites",
        icon: "icons/cloud/sprites.svg",
        offer: "Create a Fly sprite in Settings",
        noun: "sprite",
        command: plain,
        deletion: Some(Deletion {
            noun: |_| "sprite",
            leftovers: ", with its files and checkpoints,",
            run: crate::sprites::destroy_saved,
        }),
        image: None,
        provision: Some(crate::sprites::setup::provision),
    },
    // keep-sorted end
}
