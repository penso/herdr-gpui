//! Starred themes: the star on each picker row, and saving the list.
use super::ThemePicker;
use crate::{
    HerdrWindow,
    config::{
        Config,
        favorite_themes::{self, FavoriteEdit},
    },
};
use gpui::{prelude::*, *};

impl ThemePicker {
    /// Regroups the list around the selected row, keeping it selected.
    fn regroup(&mut self, favorites: &[String]) {
        let selected = self.filtered.get(self.selected).cloned();
        self.group(favorites);
        if let Some(index) =
            selected.and_then(|selected| self.filtered.iter().position(|name| *name == selected))
        {
            self.selected = index;
        }
    }
}

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

    /// Stars or unstars `name`, queues the change to be written, and
    /// regroups the list, scrolling to where the theme moved so its star
    /// stays in reach. False while a theme is being applied, when the list
    /// stays as it is.
    pub(super) fn toggle_favorite_theme(&mut self, name: &str) -> bool {
        let Some(picker) = &mut self.menu.themes else {
            return false;
        };
        if picker.accepting {
            return false;
        }
        let favorites = &mut self.config.favorite_themes;
        let starred = !favorites.iter().any(|favorite| favorite == name);
        let edit = FavoriteEdit {
            name: name.to_owned(),
            starred,
        };
        favorite_themes::apply(favorites, std::slice::from_ref(&edit));
        favorite_themes::queue(&mut picker.favorite_edits, edit);
        picker.regroup(favorites);
        if let Some(index) = picker.filtered.iter().position(|row| row == name) {
            picker.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
        true
    }

    fn save_favorite_themes(&mut self, cx: &mut Context<Self>) {
        self.save_favorite_themes_with(|edits| Config::save_favorite_themes(&edits), cx);
    }

    /// Writes the queued stars off the UI thread, each batch against the
    /// file's latest list, so another window's stars survive. Without a
    /// reload: one would replace the theme the picker is previewing. Stars
    /// clicked while a write runs are written by the same loop after it.
    pub(super) fn save_favorite_themes_with(
        &mut self,
        save: impl Fn(Vec<FavoriteEdit>) -> crate::Result<Vec<String>> + Clone + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &mut self.menu.themes else {
            return;
        };
        if picker.favorites_saving || picker.favorite_edits.is_empty() {
            return;
        }
        picker.favorites_saving = true;
        let mut edits = std::mem::take(&mut picker.favorite_edits);
        cx.spawn(async move |this, cx| {
            loop {
                let save = save.clone();
                let sent = edits.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { save(sent) })
                    .await;
                let next = this.update(cx, |this, cx| this.finish_favorite_save(edits, result, cx));
                match next {
                    Ok(Some(queued)) => edits = queued,
                    Ok(None) | Err(_) => break,
                }
            }
        })
        .detach();
    }

    /// Records a finished write of `sent` and returns the stars to write
    /// next, if any were clicked meanwhile.
    pub(super) fn finish_favorite_save(
        &mut self,
        sent: Vec<FavoriteEdit>,
        result: crate::Result<Vec<String>>,
        cx: &mut Context<Self>,
    ) -> Option<Vec<FavoriteEdit>> {
        let picker = self.menu.themes.as_mut()?;
        cx.notify();
        match result {
            Ok(mut names) => {
                picker.favorites_error = None;
                // The file holds other windows' stars too; stars clicked
                // during the write are not in it yet.
                favorite_themes::apply(&mut names, &picker.favorite_edits);
                self.config.favorite_themes = names;
                picker.regroup(&self.config.favorite_themes);
            }
            Err(error) => {
                tracing::warn!(%error, "Could not save favorite themes");
                picker.favorites_error = Some(format!("Could not save favorite themes: {error}"));
                // The next star retries these, unless it clicked the same
                // theme again; the loop stops so a failing file is not
                // rewritten on its own.
                let newer = std::mem::replace(&mut picker.favorite_edits, sent);
                for edit in newer {
                    favorite_themes::queue(&mut picker.favorite_edits, edit);
                }
                picker.favorites_saving = false;
                return None;
            }
        }
        if picker.favorite_edits.is_empty() {
            picker.favorites_saving = false;
            return None;
        }
        Some(std::mem::take(&mut picker.favorite_edits))
    }
}
