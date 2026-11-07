//! All of Sheprd in a window Moonframe lays out from `app/main.luau`, so the
//! layout around it can change without a rebuild.

use {
    moonframe::{
        HostView, Moonframe, ViewInit,
        gpui_kit::{prelude::*, *},
    },
    std::{cell::Cell, process::ExitCode, rc::Rc},
};

fn main() -> ExitCode {
    // Read before the event loop, as the app's own startup does.
    let startup = Rc::new(Cell::new(Some(sheprd_gpui::embed::prepare())));
    Moonframe::new("sheprd-moonframe")
        .title("Sheprd")
        .app_id("dev.sheprd.moonframe")
        .version(env!("CARGO_PKG_VERSION"))
        .app_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/app"))
        // The sparkles: ask an agent to change the layout or add something.
        .keep(["feedback"])
        .view("sheprd", move |_: &ViewInit, window, cx| {
            SheprdPane::new(startup.take(), window, cx)
        })
        .run()
}

/// The main view, in the pane Luau placed it.
struct SheprdPane {
    view: AnyView,
}

impl SheprdPane {
    fn new(
        startup: Option<sheprd_gpui::embed::Startup>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = sheprd_gpui::embed::view(startup, window, cx);
        #[cfg(feature = "drive")]
        drive(&view, window, cx);
        Self { view }
    }
}

impl HostView for SheprdPane {}

impl Render for SheprdPane {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // `Terminal` keeps Moonframe's Escape off: it belongs to the terminal.
        div()
            .key_context("Terminal")
            .size_full()
            .child(self.view.clone())
    }
}

/// Sends `SPIKE_DRIVE` to the view: `keys:a b enter` or `drag:x1,y1,x2,y2`.
#[cfg(feature = "drive")]
fn drive(view: &AnyView, window: &Window, cx: &mut App) {
    use sheprd_gpui::embed::Drive;
    let Ok(spec) = std::env::var("SPIKE_DRIVE") else {
        return;
    };
    let input = if let Some(keys) = spec.strip_prefix("keys:") {
        Drive::Keys(keys.to_owned())
    } else {
        let numbers: Vec<f32> = spec
            .trim_start_matches("drag:")
            .split(',')
            .filter_map(|number| number.parse().ok())
            .collect();
        let [x1, y1, x2, y2] = numbers[..] else {
            return;
        };
        Drive::Drag {
            from: (x1, y1),
            to: (x2, y2),
        }
    };
    let delay = std::env::var("SPIKE_DRIVE_DELAY")
        .ok()
        .and_then(|seconds| seconds.parse().ok())
        .unwrap_or(7);
    sheprd_gpui::embed::drive(
        view.clone(),
        input,
        std::time::Duration::from_secs(delay),
        window,
        cx,
    );
}
