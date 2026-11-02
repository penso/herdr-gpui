//! Notifications: delivery, in-app toasts, and sound.
// i18n: migrated
use super::{
    SettingsWindow, TOAST_POSITIONS, on_off, on_off_choices, on_value, position_name,
    position_names, ui,
};
use crate::settings_window::inputs::{DropdownField, NumberField};
use crate::{
    config::preferences::Preference,
    herdr_settings::{Edit, ToastDelivery},
    i18n::fl,
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
        let position_names = position_names(&TOAST_POSITIONS);
        let base_position = shared.map_or(
            crate::config::NotificationConfig::default().position,
            |shared| shared.toast_position,
        );
        let base_position = position_name(&TOAST_POSITIONS, &base_position);
        let sound = shared.is_some_and(|shared| shared.sound_enabled);
        let finished = self.config.notifications.finished;
        let source_alive = self.source.upgrade().is_some();
        div()
            .flex()
            .flex_col()
            .child(ui::section(
                theme,
                &fl!("settings-notifications-section-delivery"),
                Some(ui::Source::Herdr),
            ))
            .child(
                self.gate_shared(
                    ui::row(theme, "notif-delivery", fl!("settings-notif-delivery"))
                        .description(fl!("settings-notif-delivery-description")),
                )
                .control(ui::segmented(
                    theme,
                    "notif-delivery-control",
                    &[
                        (
                            ToastDelivery::Off,
                            fl!("settings-notifications-delivery-off"),
                        ),
                        (
                            ToastDelivery::Herdr,
                            fl!("settings-notifications-delivery-herdr"),
                        ),
                        (
                            ToastDelivery::Terminal,
                            fl!("settings-notifications-delivery-terminal"),
                        ),
                        (
                            ToastDelivery::System,
                            fl!("settings-notifications-delivery-system"),
                        ),
                    ],
                    shared.map(|shared| shared.toast_delivery),
                    ready,
                    on_value(cx, |this, mode, _, cx| {
                        this.save_shared(Edit::Toasts(mode), cx)
                    }),
                )),
            )
            .child(
                ui::row(theme, "notif-finished", fl!("settings-notif-finished"))
                    .tag(ui::Source::HerdrUi)
                    .description(fl!("settings-notifications-finished-description"))
                    .switch(self.preference_toggle(
                        "notif-finished-toggle",
                        finished,
                        Preference::NotificationFinished(!finished),
                        cx,
                    )),
            )
            .child(ui::section(
                theme,
                &fl!("settings-notifications-section-toasts"),
                Some(ui::Source::HerdrUi),
            ))
            .child(
                ui::row(theme, "notif-show", fl!("settings-notif-show")).control(
                    self.override_dropdown(
                        DropdownField::ToastEnabled,
                        &on_off(shared_enabled),
                        &on_off_choices(),
                        overrides.enabled().map(|on| usize::from(!on)),
                        cx,
                    ),
                ),
            )
            .child(self.delay_row(cx))
            .child(
                ui::row(theme, "notif-position", fl!("settings-notif-position"))
                    .description(fl!("settings-notifications-position-description"))
                    .control(self.override_dropdown(
                        DropdownField::ToastPosition,
                        &base_position,
                        &position_names,
                        position,
                        cx,
                    )),
            )
            .child(ui::section(
                theme,
                &fl!("settings-notifications-section-sound"),
                None,
            ))
            .child(
                self.gate_shared(
                    ui::row(theme, "notif-sound", fl!("settings-notif-sound"))
                        .tag(ui::Source::Herdr)
                        .description(if source_alive {
                            fl!("settings-notifications-sound-description")
                        } else {
                            fl!("settings-notifications-sound-description-no-window")
                        }),
                )
                .control(
                    ui::button(
                        theme,
                        "notif-sound-test",
                        fl!("settings-notifications-sound-test"),
                        source_alive,
                    )
                    .when(source_alive, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| {
                            if let Some(source) = this.source.upgrade() {
                                source.read(cx).sound.preview();
                            }
                        }))
                    }),
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
        ui::row(&self.theme, "notif-delay", fl!("settings-notif-delay"))
            .description(fl!("settings-notifications-delay-description"))
            .control(ui::number(
                &self.theme,
                ui::Number {
                    id: NumberField::ToastDelay.selector(),
                    value,
                    range: NumberField::ToastDelay.range(),
                    unit: ui::Unit::Seconds,
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
                    ui::button(
                        &self.theme,
                        "notif-delay-reset",
                        fl!("settings-notifications-delay-reset"),
                        true,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.save_preference(Preference::NotificationDelay(None), cx)
                    })),
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
        ui::row(&self.theme, "notif-volume", fl!("settings-notif-volume"))
            .tag(ui::Source::HerdrUi)
            .description(fl!("settings-notif-volume-description"))
            .control(ui::number(
                &self.theme,
                ui::Number {
                    id: NumberField::SoundVolume.selector(),
                    value,
                    range: NumberField::SoundVolume.range(),
                    unit: ui::Unit::Percent,
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
