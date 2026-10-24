use super::*;
use crate::{system_load::Sample, usage::Host};

fn gpu_sample() -> Sample {
    Sample {
        cpu: Some(12.),
        memory: Some(crate::system_load::Memory {
            used: 4 << 30,
            total: 16 << 30,
        }),
        gpu: Some(crate::system_load::Gpu {
            busy: Some(45.),
            memory: Some(crate::system_load::Memory {
                used: 9 << 30,
                total: 24 << 30,
            }),
            count: 1,
        }),
        ..Sample::default()
    }
}

/// A host with a GPU gets a second load line under CPU and memory, aligned
/// with it, once `show_sidebar_gpu_load` is on, and a status bar item once
/// `show_gpu_load` is; each setting works without the other.
#[gpui::test]
fn a_host_with_a_gpu_adds_a_line_and_a_status_item_once_shown(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot(1)));
        view.config.show_system_load = true;
        view.config.show_gpu_load = true;
        view.config.show_sidebar_gpu_load = true;
        // A density with detail lines, which gives load lines of its own.
        view.config.layout.mode =
            crate::config::LayoutMode::from(crate::config::Density::Comfortable);
        view.system_load.record(Host::Local, gpu_sample());
        // Host headers show once there is more than one host.
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:test".into(),
            "Remote".into(),
            ConnectTarget::Ssh {
                target: "me@100.84.59.116".into(),
                session: "default".into(),
            },
            true,
        );
        remote.live.snapshot = view.live.snapshot.clone();
        view.endpoints.push(remote);
        // The status bar repeats the load selectors; it is checked below.
        view.status_bar_visible = false;
        view
    });
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let header = cx.debug_bounds("host-local").unwrap();
    let gpu = cx.debug_bounds("host-gpu-local").unwrap();
    assert!(header.bottom() >= gpu.bottom(), "{gpu:?} inside {header:?}");
    let (cpu, busy) = (
        cx.debug_bounds("system-load-cpu").unwrap(),
        cx.debug_bounds("system-load-gpu").unwrap(),
    );
    assert!(busy.top() >= cpu.bottom(), "{busy:?} below {cpu:?}");
    assert!((cpu.left() - busy.left()).abs() < px(1.), "columns align");
    let (memory, vram) = (
        cx.debug_bounds("system-load-memory").unwrap(),
        cx.debug_bounds("system-load-vram").unwrap(),
    );
    assert!(
        (memory.left() - vram.left()).abs() < px(1.),
        "columns align"
    );
    let with_gpu = header.size.height;

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.status_bar_visible = true;
            cx.notify();
        });
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("system-load").is_some());
    assert!(cx.debug_bounds("gpu-load").is_some());

    // The status bar's setting leaves the sidebar line alone.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.show_gpu_load = false;
            cx.notify();
        });
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("gpu-load").is_none());
    assert!(cx.debug_bounds("host-gpu-local").is_some());
    // And the sidebar's leaves the status bar alone.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.show_gpu_load = true;
            view.config.show_sidebar_gpu_load = false;
            cx.notify();
        });
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("host-gpu-local").is_none());
    assert!(cx.debug_bounds("gpu-load").is_some());
    assert!(cx.debug_bounds("host-local").unwrap().size.height < with_gpu);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.show_sidebar_gpu_load = true;
            cx.notify();
        });
        full_draw(window, cx).clear(cx);
    });

    // Without a GPU the row loses the line and the status bar its item.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.system_load.record(
                Host::Local,
                Sample {
                    gpu: None,
                    ..gpu_sample()
                },
            );
            cx.notify();
        });
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("host-gpu-local").is_none());
    assert!(cx.debug_bounds("gpu-load").is_none());
    assert!(cx.debug_bounds("host-local").unwrap().size.height < with_gpu);
}
