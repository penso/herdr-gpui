//! Starred themes: the star on each picker row, and saving the list.
use crate::{
    HerdrWindow,
    config::{Config, preferences::Preference},
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// The star at a row's end. Clicking it stars or unstars the theme
    /// without previewing or saving it as the theme.
    pub(super) fn favorite_star(
        &self,
        index: usize,
        name: &str,
        favorite: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let name = name.to_owned();
        let (path, color) = if favorite {
            (crate::icons::STARRED, self.theme.palette[3])
        } else {
            (crate::icons::UNSTARRED, self.theme.muted)
        };
        div()
            .id(("theme-star", index))
            .debug_selector(move || format!("theme-star-{index}"))
            .role(Role::Button)
            .aria_label(if favorite {
                "Unstar theme"
            } else {
                "Star theme"
            })
            .flex_none()
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::CONTROL))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(self.theme.surface)))
            .child(crate::icons::icon(path, 14., color))
            .on_click(cx.listener(move |this, _, _, cx| {
                // The row under the star applies its theme on click.
                cx.stop_propagation();
                if this.toggle_favorite_theme(&name) {
                    this.save_favorite_themes(cx);
                }
                cx.notify();
            }))
    }

    /// Stars or unstars `name`, regrouping the list around the selected row
    /// without scrolling it. False while a theme is being applied, when the
    /// list stays as it is.
    pub(super) fn toggle_favorite_theme(&mut self, name: &str) -> bool {
        let Some(picker) = &mut self.menu.themes else {
            return false;
        };
        if picker.accepting {
            return false;
        }
        let favorites = &mut self.config.favorite_themes;
        match favorites.iter().position(|favorite| favorite == name) {
            Some(index) => {
                favorites.remove(index);
            }
            None => favorites.push(name.to_owned()),
        }
        let selected = picker.filtered.get(picker.selected).cloned();
        picker.group(favorites);
        if let Some(index) =
            selected.and_then(|selected| picker.filtered.iter().position(|name| *name == selected))
        {
            picker.selected = index;
        }
        true
    }

    fn save_favorite_themes(&mut self, cx: &mut Context<Self>) {
        self.save_favorite_themes_with(
            |names| Config::save_preference(Preference::FavoriteThemes(names)),
            cx,
        );
    }

    /// Writes the starred list off the UI thread. Without a reload: one
    /// would replace the theme the picker is previewing, so the window keeps
    /// the list it already changed. A star clicked while a write runs is
    /// written by the same loop once that write ends.
    pub(super) fn save_favorite_themes_with(
        &mut self,
        save: impl Fn(Vec<String>) -> crate::Result<()> + Clone + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &mut self.menu.themes else {
            return;
        };
        if picker.favorites_saving {
            picker.favorites_pending = true;
            return;
        }
        picker.favorites_saving = true;
        let mut names = self.config.favorite_themes.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let save = save.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { save(names) })
                    .await;
                let next = this.update(cx, |this, cx| this.finish_favorite_save(result, cx));
                match next {
                    Ok(Some(latest)) => names = latest,
                    Ok(None) | Err(_) => break,
                }
            }
        })
        .detach();
    }

    /// Records a finished write and returns the list to write next, if a
    /// star changed it meanwhile.
    pub(super) fn finish_favorite_save(
        &mut self,
        result: crate::Result<()>,
        cx: &mut Context<Self>,
    ) -> Option<Vec<String>> {
        let picker = self.menu.themes.as_mut()?;
        if let Err(error) = result {
            tracing::warn!(%error, "Could not save favorite themes");
            picker.error = Some(format!("Could not save favorite themes: {error}"));
            cx.notify();
        }
        if std::mem::take(&mut picker.favorites_pending) {
            return Some(self.config.favorite_themes.clone());
        }
        picker.favorites_saving = false;
        None
    }
}
