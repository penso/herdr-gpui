//! Notifications: delivery, in-app toasts, and sound.
use super::{SettingsWindow, TOAST_POSITIONS, on_off, on_value, ui};
use crate::settings_window::inputs::{DropdownField, NumberField};
use crate::{
    config::preferences::Preference,
    herdr_settings::{Edit, ToastDelivery},
};
use gpui::{prelude::*, *};

impl SettingsWindow {
    pub(in crate::settings_window) fn render_notifications(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let ready = self.controls_shared_ready();
        let shared = self.shared.as_ref();
        let overrides = self.config.notification_overrides;
        let shared_enabled = shared.is_some_and(|s| s.toast_delivery == ToastDelivery::Herdr);
        let position = overrides
            .position()
            .and_then(|p| TOAST_POSITIONS.iter().position(|(c, _)| *c == p));
        let position_names = TOAST_POSITIONS.map(|(_, name)| name);
        let base_position = shared.map_or(
            crate::config::NotificationConfig::default().position,
            |shared| shared.toast_position,
        );
        let base_position = TOAST_POSITIONS
            .iter()
            .find_map(|(p, name)| (*p == base_position).then_some(*name))
            .unwrap_or_default();
        let sound = shared.is_some_and(|shared| shared.sound_enabled);
        let finished = self.config.notifications.finished;
        let source_alive = self.source.upgrade().is_some();
        div()
            .flex()
            .flex_col()
            .child(ui::section(theme, "Delivery", Some(ui::Source::Herdr)))
            .child(
                self.gate_shared(
                    ui::row(theme, "notif-delivery", "Deliver agent notifications").description(
                        "Herdr shows them in-app. System uses OS notifications; clicking one \
                         opens its pane. Terminal is for other clients only.",
                    ),
                )
                .control(ui::segmented(
                    theme,
                    "notif-delivery-control",
                    &[
                        (ToastDelivery::Off, "Off"),
                        (ToastDelivery::Herdr, "Herdr"),
                        (ToastDelivery::Terminal, "Terminal"),
                        (ToastDelivery::System, "System"),
                    ],
                    shared.map(|shared| shared.toast_delivery),
                    ready,
                    on_value(cx, |this, mode, _, cx| {
                        this.save_shared(Edit::Toasts(mode), cx)
                    }),
                )),
            )
            .child(
                ui::row(theme, "notif-finished", "Finished notifications")
                    .tag(ui::Source::HerdrUi)
                    .description(
                        "Off drops the notice at the end of each agent turn, in-app and from \
                         the system; \u{201c}needs attention\u{201d} still shows.",
                    )
                    .switch(self.preference_toggle(
                        "notif-finished-toggle",
                        finished,
                        Preference::NotificationFinished(!finished),
                        cx,
                    )),
            )
            .child(ui::section(
                theme,
                "In-app toasts",
                Some(ui::Source::HerdrUi),
            ))
            .child(
                ui::row(theme, "notif-show", "Show toasts").control(self.override_dropdown(
                    DropdownField::ToastEnabled,
                    on_off(shared_enabled),
                    &["On", "Off"],
                    overrides.enabled().map(|on| usize::from(!on)),
                    cx,
                )),
            )
            .child(self.delay_row(cx))
            .child(
                ui::row(theme, "notif-position", "Position")
                    .description(
                        "Controls show effective values; a managed-file override still \
                         takes precedence.",
                    )
                    .control(self.override_dropdown(
                        DropdownField::ToastPosition,
                        base_position,
                        &position_names,
                        position,
                        cx,
                    )),
            )
            .child(ui::section(theme, "Sound", None))
            .child(
                self.gate_shared(
                    ui::row(theme, "notif-sound", "Play agent sounds")
                        .tag(ui::Source::Herdr)
                        .description(if source_alive {
                            "Uses Herdr's sound paths and per-agent overrides. Missing custom \
                             sounds fall back to bundled sounds."
                        } else {
                            "Uses Herdr's sound paths and per-agent overrides. Missing custom \
                             sounds fall back to bundled sounds. Open a main Herdr window to \
                             preview audio."
                        }),
                )
                .control(
                    ui::button(theme, "notif-sound-test", "Test", source_alive).when(
                        source_alive,
                        |button| {
                            button.on_click(cx.listener(|this, _, _, cx| {
                                if let Some(source) = this.source.upgrade() {
                                    source.read(cx).sound.preview();
                                }
                            }))
                        },
                    ),
                )
                .switch(
                    ui::toggle(theme, "notif-sound-toggle", sound, ready).when(ready, |toggle| {
                        toggle.on_click(cx.listener(move |this, _, _, cx| {
                            this.save_shared(Edit::Sound(!sound), cx)
                        }))
                    }),
                ),
            )
            .child(self.volume_row(cx))
    }

    /// The delay in seconds: a typed field with steppers, and a reset when the
    /// local file overrides Herdr's delay.
    fn delay_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let overridden = self.config.notification_overrides.delay_seconds().is_some();
        let on_change = on_value(cx, |this, change, _, cx| {
            this.commit_number(NumberField::ToastDelay, change, cx)
        });
        let value = self.config.notifications.delay_seconds as f32;
        let view = cx.entity().downgrade();
        ui::row(&self.theme, "notif-delay", "Delay")
            .description(
                "Wait before a toast appears, so quick turns stay quiet. Controls show \
                 effective values; a managed-file override still takes precedence.",
            )
            .control(ui::number(
                &self.theme,
                ui::Number {
                    id: NumberField::ToastDelay.selector(),
                    value,
                    range: NumberField::ToastDelay.range(),
                    unit: "s",
                    look: ui::Look::Steppers,
                    enabled: true,
                },
                self.number_edit_for(NumberField::ToastDelay),
                on_change,
                move |window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.begin_number_edit(NumberField::ToastDelay, value, window, cx)
                    });
                },
            ))
            .when(overridden, |row| {
                row.control(
                    ui::button(&self.theme, "notif-delay-reset", "Reset", true).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.save_preference(Preference::NotificationDelay(None), cx)
                        }),
                    ),
                )
            })
    }

    /// The local sound volume: a slider and a typed percentage, editable
    /// even where Herdr's shared sound settings are read-only, since it lives
    /// in this app's own file.
    fn volume_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let on_change = on_value(cx, |this, change, _, cx| {
            this.commit_number(NumberField::SoundVolume, change, cx)
        });
        let value = self.number_value(
            NumberField::SoundVolume,
            f32::from(self.config.sound.volume),
        );
        let view = cx.entity().downgrade();
        ui::row(&self.theme, "notif-volume", "Volume")
            .tag(ui::Source::HerdrUi)
            .description("Sounds and Test play at this level, even when sounds are off.")
            .control(ui::number(
                &self.theme,
                ui::Number {
                    id: NumberField::SoundVolume.selector(),
                    value,
                    range: NumberField::SoundVolume.range(),
                    unit: "%",
                    look: ui::Look::Slider,
                    enabled: true,
                },
                self.number_edit_for(NumberField::SoundVolume),
                on_change,
                move |window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.begin_number_edit(NumberField::SoundVolume, value, window, cx)
                    });
                },
            ))
    }
}
