//! The size choices in the New VM and New sandbox boxes: VM sizes with their
//! specs and price, with Other opening the VM Size field for any size Azure
//! offers, and the sandbox sizes the service takes. A choice saves its one
//! key at once, as the agent choices do. Specs and prices come from the
//! card's worker, read for the saved regions, so render only formats them.

use super::{AzureCard, SettingsWindow, VM_SIZE};
use crate::azure::{
    settings::DEFAULT_VM_SIZE,
    sizes::{
        Catalog, DEFAULT_SANDBOX_MILLICORES, SANDBOX_MILLICORES, SandboxSize, VM_CHOICES,
        ephemeral_placement, is_arm,
    },
};
use crate::config::Config;
use gpui::{prelude::*, *};

/// A choice's own width; choices wrap into columns as the window allows.
const CHOICE_WIDTH: f32 = 280.;

/// A VM size's lines: what it is, then what it costs as Spot, which new VMs
/// run as where Azure offers it, with the regular price beside it.
pub(super) fn vm_detail(catalog: Option<&Catalog>, size: &str) -> Vec<String> {
    let disk = ephemeral_placement(size)
        .is_none()
        .then(|| "No local storage: a Standard SSD disk, billed until deleted".to_owned());
    let mut lines = vm_lines(catalog, size);
    lines.extend(disk);
    lines
}

fn vm_lines(catalog: Option<&Catalog>, size: &str) -> Vec<String> {
    let Some(catalog) = catalog else {
        return vec![if is_arm(size) { "Arm" } else { "x64" }.to_owned()];
    };
    let Some(spec) = catalog.vms.get(size) else {
        return vec![format!("Not offered in {}", catalog.location)];
    };
    let price = match (catalog.spot_prices.get(size), catalog.vm_prices.get(size)) {
        (Some(spot), Some(regular)) => Some(format!(
            "Spot {} · regular {}",
            spot.text(true),
            regular.text(false)
        )),
        (Some(spot), None) => Some(format!("Spot {}", spot.text(true))),
        (None, Some(regular)) => Some(format!("Regular {}", regular.text(true))),
        (None, None) => None,
    };
    std::iter::once(spec.summary(size)).chain(price).collect()
}

fn sandbox_detail(catalog: Option<&Catalog>, size: SandboxSize) -> Vec<String> {
    vec![sandbox_line(catalog, size)]
}

fn sandbox_line(catalog: Option<&Catalog>, size: SandboxSize) -> String {
    match catalog.and_then(|catalog| catalog.sandbox_rates) {
        Some(rates) => format!(
            "{} · {} while running",
            size.summary(),
            rates.hourly(size).text(false)
        ),
        None => size.summary(),
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
        self.control_choice(
            id,
            div()
                .min_w_0()
                .child(div().truncate().child(title))
                .children(
                    detail
                        .into_iter()
                        .map(|line| div().truncate().text_color(muted).child(line)),
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
            Some(catalog) => format!(
                "New VMs keep their OS disk on the VM's local storage, so there is no disk to \
                 pay for, and auto-stop deletes an idle one: keep work in Git. They run as Spot \
                 where Azure offers it to the subscription, and Azure may delete a Spot VM when \
                 it needs the capacity back; otherwise they are billed at the regular price. \
                 Linux list prices in {}, in US dollars, as of now: Spot prices move.",
                catalog.location
            ),
            None => "Specs and prices show once the account is saved and reachable.".to_owned(),
        };
        let choices = VM_CHOICES.iter().map(|&size| {
            let title = if size == DEFAULT_VM_SIZE {
                format!("{size} (default)")
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
                "Other".into(),
                vec!["Any size the region offers".into()],
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
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .children(choices)
                    .child(other_choice),
            )
            .when(other, |section| {
                section
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .min_w_0()
                            .child(div().flex_1().min_w_0().child(card.fields[VM_SIZE].clone()))
                            .child(
                                self.cloud_button("cloud-azure-vm-size-save", "Use size", ready)
                                    .when(ready, |button| {
                                        button.on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.save_typed_vm_size(cx)
                                            }),
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
            Some(catalog) if catalog.sandbox_rates.is_some() => format!(
                "Billed per second at Container Apps rates in {} while running; a suspended \
                 sandbox is not billed for CPU or memory.",
                catalog.sandbox_location
            ),
            _ => "Memory is 2 GiB and disk 20 GiB per vCPU.".to_owned(),
        };
        let choices = SANDBOX_MILLICORES.iter().map(|&millicores| {
            let size = SandboxSize::standard(millicores);
            let title = if millicores == DEFAULT_SANDBOX_MILLICORES {
                format!("{} vCPU (default)", size.cores())
            } else {
                format!("{} vCPU", size.cores())
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
            .child(div().flex().flex_wrap().gap(px(8.)).children(choices))
    }
}
