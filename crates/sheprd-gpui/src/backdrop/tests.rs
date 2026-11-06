#![allow(clippy::unwrap_used)]
use super::*;

fn backdrop(opacity: f32) -> config::Backdrop {
    config::Backdrop {
        opacity,
        ..Default::default()
    }
}

#[test]
fn an_opaque_backdrop_paints_every_surface_as_before() {
    let theme = Theme::default();
    let fill = Fill::new(&theme, &config::Backdrop::default());
    assert!(!fill.translucent());
    for color in [theme.background, theme.surface, 0x123456] {
        assert_eq!(fill.base(color), rgb(color).into());
        assert_eq!(fill.panel(color), rgb(color).into());
        assert_eq!(fill.over(color, color), rgb(color).into());
    }
    assert_eq!(fill.ground(), rgb(theme.background).into());
    // A picture under an opaque ground would never show, so none is asked for.
    let hidden = config::Backdrop {
        image: Some("/pictures/a.png".into()),
        ..Default::default()
    };
    assert_eq!(
        appearance(&hidden, true),
        WindowBackgroundAppearance::Opaque
    );
}

#[test]
fn a_translucent_layer_on_its_own_color_paints_nothing() {
    let theme = Theme::default();
    let fill = Fill::new(&theme, &backdrop(0.8));
    assert!(fill.translucent());
    let translucent = |color| Hsla::from(rgb(color)).opacity(0.8);
    assert_eq!(fill.ground(), translucent(theme.background));
    // Directly on the ground, the theme's background would stack its alpha.
    assert_eq!(fill.base(theme.background), transparent_black());
    // Anything else on a translucent surface is a mark, at the opacity
    // squared, so it shows without turning the window solid there.
    let mark = |color| Hsla::from(rgb(color)).opacity(0.8 * 0.8);
    assert_eq!(fill.base(theme.surface), mark(theme.surface));
    // Tabs on their strip: the strip's own color clears, a selection shows.
    assert_eq!(fill.over(theme.surface, theme.surface), transparent_black());
    assert_eq!(fill.over(theme.surface, 0x123456), mark(0x123456));
    assert_eq!(fill.panel(theme.background), translucent(theme.background));
}

#[test]
fn the_window_stays_opaque_under_a_picture_and_blurs_only_the_desktop() {
    let mut translucent = backdrop(0.5);
    assert_eq!(
        appearance(&translucent, false),
        WindowBackgroundAppearance::Transparent
    );
    translucent.blur = true;
    assert_eq!(
        appearance(&translucent, false),
        WindowBackgroundAppearance::Blurred
    );
    assert_eq!(
        appearance(&translucent, true),
        WindowBackgroundAppearance::Opaque
    );
}

#[test]
fn mica_replaces_blur_on_windows_only() {
    let mut mica = backdrop(0.5);
    mica.blur = true;
    mica.material = config::Material::Mica;
    let expected = if cfg!(windows) {
        WindowBackgroundAppearance::MicaBackdrop
    } else {
        WindowBackgroundAppearance::Blurred
    };
    assert_eq!(appearance(&mica, false), expected);
    // Without blur it is plain translucency, and a picture covers it.
    mica.blur = false;
    assert_eq!(
        appearance(&mica, false),
        WindowBackgroundAppearance::Transparent
    );
    mica.blur = true;
    assert_eq!(appearance(&mica, true), WindowBackgroundAppearance::Opaque);
}

fn pixel() -> RenderImage {
    texture(RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 4])))
}

#[test]
fn a_picture_shows_only_the_latest_path_and_releases_what_it_replaced() {
    let (a, b) = (PathBuf::from("/a.png"), PathBuf::from("/b.png"));
    let mut picture = Picture::default();
    assert!(!picture.covers());
    assert_eq!(picture.want(Some(a.clone())), Some(a.clone()));
    // Asking again for the same path starts nothing.
    assert_eq!(picture.want(Some(a.clone())), None);
    assert!(picture.covers() && picture.texture().is_none());
    picture.finish(a.clone(), Ok(pixel()));
    let first = picture.texture().unwrap();
    // The config moved on before a decode finished: the late one is dropped.
    assert_eq!(picture.want(Some(b.clone())), Some(b.clone()));
    let released = picture.take_released();
    assert_eq!(released.len(), 1);
    assert!(Arc::ptr_eq(&released[0], &first));
    picture.finish(a, Ok(pixel()));
    assert!(picture.texture().is_none() && picture.covers());
    picture.finish(b.clone(), Ok(pixel()));
    assert!(picture.texture().is_some());
    // Removing the picture releases its texture and uncovers the desktop.
    assert_eq!(picture.want(None), None);
    assert_eq!(picture.take_released().len(), 1);
    assert!(!picture.covers() && picture.texture().is_none());
}

#[test]
fn a_failed_picture_uncovers_the_desktop_without_retrying() {
    let path = PathBuf::from("/missing.png");
    let mut picture = Picture::default();
    picture.want(Some(path.clone()));
    picture.finish(path.clone(), Err(Error::NotAFile { path: path.clone() }));
    assert!(!picture.covers() && picture.texture().is_none());
    assert_eq!(picture.want(Some(path)), None);
}

#[test]
fn decoding_scales_large_pictures_down_and_swaps_to_bgra() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("wide.png");
    RgbaImage::from_pixel(MAX_TEXTURE_SIDE * 2, 10, image::Rgba([10, 20, 30, 255])).save(&path)?;
    let decoded = decode(&path)?;
    let size = decoded.size(0);
    assert_eq!((size.width.0, size.height.0), (MAX_TEXTURE_SIDE as i32, 5));
    assert_eq!(&decoded.as_bytes(0).unwrap()[..4], &[30, 20, 10, 255]);
    // A picture that already fits keeps its size.
    let small = directory.path().join("small.png");
    RgbaImage::new(3, 2).save(&small)?;
    let size = decode(&small)?.size(0);
    assert_eq!((size.width.0, size.height.0), (3, 2));
    Ok(())
}

#[test]
fn decoding_refuses_what_is_not_a_picture() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    assert!(matches!(
        decode(directory.path()),
        Err(Error::NotAFile { .. })
    ));
    let missing = decode(&directory.path().join("missing.png"));
    assert!(matches!(&missing, Err(Error::Read { source, .. })
        if source.kind() == std::io::ErrorKind::NotFound));
    let text = directory.path().join("notes.png");
    fs::write(&text, "not a picture")?;
    let error = decode(&text).err().unwrap();
    assert!(matches!(error, Error::Decode { .. }));
    // The cause reaches the message people read, not only the source chain.
    let source = std::error::Error::source(&error).unwrap().to_string();
    assert!(error.to_string().ends_with(&source), "{error}");
    let missing = decode(&directory.path().join("missing.png")).err().unwrap();
    let source = std::error::Error::source(&missing).unwrap().to_string();
    assert!(missing.to_string().ends_with(&source), "{missing}");
    let large = directory.path().join("large.png");
    let file = fs::File::create(&large)?;
    file.set_len(MAX_FILE_BYTES + 1)?;
    assert!(matches!(decode(&large), Err(Error::TooLarge { .. })));
    Ok(())
}

/// A FIFO with no writer would block an open forever; it must be refused
/// before then, so neither the decode nor a Settings save waiting on it hangs.
#[cfg(unix)]
#[test]
fn decoding_refuses_a_fifo_without_waiting_for_a_writer() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let fifo = directory.path().join("picture.png");
    let status = std::process::Command::new("mkfifo").arg(&fifo).status()?;
    anyhow::ensure!(status.success(), "mkfifo failed");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(decode(&fifo).map(|_| ()));
    });
    let result = receiver.recv_timeout(std::time::Duration::from_secs(5))?;
    assert!(matches!(result, Err(Error::NotAFile { .. })));
    Ok(())
}
