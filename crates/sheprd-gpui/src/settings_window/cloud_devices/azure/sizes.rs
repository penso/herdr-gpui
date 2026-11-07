//! The size choices in the New VM and New sandbox boxes: VM sizes with their
//! specs and price, with Other opening the VM Size field for any size Azure
//! offers, and the sandbox sizes the service takes. A choice saves its one
//! key at once, as the agent choices do. Specs and prices come from the
//! card's worker, read for the saved regions, so render only formats them.

use super::{AzureCard, SettingsWindow, VM_SIZE};
use crate::azure::{
    settings::DEFAULT_VM_SIZE,
    sizes::{
        Catalog, DEFAULT_SANDBOX_MILLICORES, DeviceSize, Price, SANDBOX_MILLICORES, SandboxSize,
        VM_CHOICES, VmSpec, ephemeral_placement, is_arm,
    },
};
use crate::direction::Logical;
use crate::{config::Config, i18n::fl};
use gpui::{prelude::*, *};

/// A choice's own width; choices wrap into columns as the window allows.
const CHOICE_WIDTH: f32 = 280.;

/// A VM size's lines: what it is, then what it costs as Spot, which new VMs
/// run as where Azure offers it, with the regular price beside it.
pub(super) fn vm_detail(catalog: Option<&Catalog>, size: &str) -> Vec<String> {
    let disk = ephemeral_placement(size)
        .is_none()
        .then(|| fl!("settings-cloud-azure-size-no-local-disk"));
    let mut lines = vm_lines(catalog, size);
    lines.extend(disk);
    lines
}

fn vm_lines(catalog: Option<&Catalog>, size: &str) -> Vec<String> {
    let Some(catalog) = catalog else {
        return vec![arch(size)];
    };
    let Some(spec) = catalog.vms.get(size) else {
        return vec![fl!(
            "settings-cloud-azure-size-not-offered",
            location = catalog.location.clone()
        )];
    };
    let price = match (catalog.spot_prices.get(size), catalog.vm_prices.get(size)) {
        (Some(spot), Some(regular)) => Some(fl!(
            "settings-cloud-azure-price-spot-regular",
            spot = price(*spot, true),
            regular = price(*regular, false)
        )),
        (Some(spot), None) => Some(fl!(
            "settings-cloud-azure-price-spot",
            spot = price(*spot, true)
        )),
        (None, Some(regular)) => Some(fl!(
            "settings-cloud-azure-price-regular",
            regular = price(*regular, true)
        )),
        (None, None) => None,
    };
    std::iter::once(vm_spec(*spec, size)).chain(price).collect()
}

/// Arm or x64.
fn arch(size: &str) -> String {
    fl!(
        "azure-sizes-arch",
        arch = if is_arm(size) { "arm" } else { "x64" }
    )
}

/// `4 GiB`, or `1.5 GiB` when not whole.
fn memory(mib: u64) -> String {
    let gib = if mib.is_multiple_of(1024) {
        (mib / 1024).to_string()
    } else {
        format!("{:.1}", mib as f64 / 1024.0)
    };
    fl!("azure-sizes-gib", gib = gib)
}

fn vm_spec(spec: VmSpec, size: &str) -> String {
    fl!(
        "azure-sizes-vm",
        cores = spec.cores,
        memory = memory(spec.memory_mb),
        arch = arch(size)
    )
}

fn sandbox_size(size: SandboxSize) -> String {
    fl!(
        "azure-sizes-sandbox",
        cores = size.cores(),
        memory = memory(u64::from(size.memory_mib))
    )
}

/// `$0.0095/hr`, with `· ~$7/mo` when `month`: four decimals under a
/// dollar, two above.
fn price(price: Price, month: bool) -> String {
    let amount = if price.0 < 1.0 {
        format!("{:.4}", price.0)
    } else {
        format!("{:.2}", price.0)
    };
    if !month {
        return fl!("azure-sizes-hourly", amount = amount);
    }
    let monthly = format!("{:.0}", price.monthly());
    fl!(
        "azure-sizes-hourly-monthly",
        amount = amount,
        monthly = monthly
    )
}

/// A saved machine's size as its row in the card lists it.
pub(super) fn device_size(size: &DeviceSize) -> String {
    match size {
        DeviceSize::Vm {
            size,
            spec: Some(spec),
            spot,
        } => fl!(
            "azure-sizes-device-vm",
            size = size.as_str(),
            spec = vm_spec(*spec, size),
            spot = if *spot { "yes" } else { "no" }
        ),
        DeviceSize::Vm {
            size,
            spec: None,
            spot,
        } => fl!(
            "azure-sizes-device-vm-unlisted",
            size = size.as_str(),
            spot = if *spot { "yes" } else { "no" }
        ),
        DeviceSize::Sandbox(size) => sandbox_size(*size),
    }
}

fn sandbox_detail(catalog: Option<&Catalog>, size: SandboxSize) -> Vec<String> {
    vec![sandbox_line(catalog, size)]
}

fn sandbox_line(catalog: Option<&Catalog>, size: SandboxSize) -> String {
    match catalog.and_then(|catalog| catalog.sandbox_rates) {
        Some(rates) => fl!(
            "settings-cloud-azure-sandbox-price",
            size = sandbox_size(size),
            disk = size.disk_gib(),
            price = price(rates.hourly(size), false)
        ),
        None => fl!(
            "settings-cloud-azure-sandbox-size",
            size = sandbox_size(size),
            disk = size.disk_gib()
        ),
    }
}

impl SettingsWindow {
    fn choose_vm_size(&mut self, size: Option<&'static str>, cx: &mut Context<Self>) {
        let Some(card) = &mut self.azure_card else {
            return;
        };
        match size {
            Some(size) => {
                card.vm_other = false;
                card.fields[VM_SIZE].update(cx, |input, cx| input.set_text_selected(size, cx));
                self.save_azure_key("azure.vm_size", size.to_owned(), cx);
            }
            None => card.vm_other = true,
        }
        cx.notify();
    }

    /// Save the size typed under Other; an empty field restores the default.
    fn save_typed_vm_size(&mut self, cx: &mut Context<Self>) {
        let Some(card) = &self.azure_card else {
            return;
        };
        let size = card.fields[VM_SIZE].read(cx).text().trim().to_owned();
        self.save_azure_key("azure.vm_size", size, cx);
    }

    fn choose_sandbox_size(&mut self, size: SandboxSize, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.azure_card {
            card.sandbox_cpu = size.cores();
        }
        self.save_azure_key("azure.sandbox_cpu", size.cores(), cx);
        cx.notify();
    }

    /// Save one `[azure]` key, leaving the card's other fields as typed.
    fn save_azure_key(&mut self, key: &'static str, value: String, cx: &mut Context<Self>) {
        self.save_native(
            move || {
                if value.is_empty() {
                    Config::unset_key(key).map(drop)
                } else {
                    Config::set_key(key, toml_edit::Value::from(value))
                }
            },
            cx,
        );
    }

    fn size_choice(
        &self,
        id: SharedString,
        title: String,
        detail: Vec<String>,
        selected: bool,
        enabled: bool,
    ) -> Stateful<Div> {
        let muted = rgb(self.theme.muted);
        // Lines wrap rather than cut off, as a translation may run longer.
        self.control_choice(
            id,
            div().min_w_0().child(div().child(title)).children(
                detail
                    .into_iter()
                    .map(|line| div().text_color(muted).child(line)),
            ),
            selected,
            enabled,
        )
        .flex_grow(1.)
        .flex_basis(px(CHOICE_WIDTH))
    }

    pub(super) fn render_vm_sizes(&self, card: &AzureCard, cx: &mut Context<Self>) -> Div {
        let ready = !self.busy();
        let catalog = card.catalog();
        let typed = card.fields[VM_SIZE].read(cx).text().trim().to_owned();
        let vm_size = if typed.is_empty() {
            DEFAULT_VM_SIZE.to_owned()
        } else {
            typed
        };
        let other = card.vm_other || !VM_CHOICES.contains(&vm_size.as_str());
        let note = match catalog {
            Some(catalog) => fl!(
                "settings-cloud-azure-vm-sizes-note",
                location = catalog.location.clone()
            ),
            None => fl!("settings-cloud-azure-sizes-unknown"),
        };
        let choices = VM_CHOICES.iter().map(|&size| {
            let title = if size == DEFAULT_VM_SIZE {
                fl!("settings-cloud-default-choice", choice = size)
            } else {
                size.to_owned()
            };
            self.size_choice(
                SharedString::from(format!("cloud-azure-vm-size-{size}")),
                title,
                vm_detail(catalog, size),
                !other && vm_size == size,
                ready,
            )
            .when(ready, |choice| {
                choice.on_click(
                    cx.listener(move |this, _, _, cx| this.choose_vm_size(Some(size), cx)),
                )
            })
        });
        let other_choice = self
            .size_choice(
                "cloud-azure-vm-size-other".into(),
                fl!("settings-cloud-azure-size-other"),
                vec![fl!("settings-cloud-azure-size-other-detail")],
                other,
                ready,
            )
            .when(ready, |choice| {
                choice.on_click(cx.listener(|this, _, _, cx| this.choose_vm_size(None, cx)))
            });

        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .min_w_0()
            .child(self.control_note(note))
            .child(
                div()
                    .row()
                    .flex_wrap()
                    .gap(px(8.))
                    .children(choices)
                    .child(other_choice),
            )
            .when(other, |section| {
                section
                    .child(
                        div()
                            .row()
                            .items_center()
                            .gap(px(8.))
                            .min_w_0()
                            .child(div().flex_1().min_w_0().child(card.fields[VM_SIZE].clone()))
                            .child(
                                self.cloud_button(
                                    "cloud-azure-vm-size-save",
                                    fl!("settings-cloud-azure-size-use"),
                                    ready,
                                )
                                .when(ready, |button| {
                                    button.on_click(
                                        cx.listener(|this, _, _, cx| this.save_typed_vm_size(cx)),
                                    )
                                }),
                            ),
                    )
                    .children(
                        vm_detail(catalog, &vm_size)
                            .into_iter()
                            .map(|line| self.control_note(line)),
                    )
            })
    }

    pub(super) fn render_sandbox_sizes(&self, card: &AzureCard, cx: &mut Context<Self>) -> Div {
        let ready = !self.busy();
        let catalog = card.catalog();
        let chosen = SandboxSize::parse(&card.sandbox_cpu)
            .unwrap_or_else(|| SandboxSize::standard(DEFAULT_SANDBOX_MILLICORES));
        let note = match catalog {
            Some(catalog) if catalog.sandbox_rates.is_some() => fl!(
                "settings-cloud-azure-sandbox-sizes-note",
                location = catalog.sandbox_location.clone()
            ),
            _ => fl!("settings-cloud-azure-sandbox-sizes-unknown"),
        };
        let choices = SANDBOX_MILLICORES.iter().map(|&millicores| {
            let size = SandboxSize::standard(millicores);
            let cores = fl!("settings-cloud-azure-vcpu", cores = size.cores());
            let title = if millicores == DEFAULT_SANDBOX_MILLICORES {
                fl!("settings-cloud-default-choice", choice = cores)
            } else {
                cores
            };
            self.size_choice(
                SharedString::from(format!("cloud-azure-sandbox-size-{millicores}")),
                title,
                sandbox_detail(catalog, size),
                chosen == size,
                ready,
            )
            .when(ready, |choice| {
                choice
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_sandbox_size(size, cx)))
            })
        });
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .min_w_0()
            .child(self.control_note(note))
            .child(div().row().flex_wrap().gap(px(8.)).children(choices))
    }
}

#[cfg(test)]
mod tests;
