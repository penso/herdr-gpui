//! Finding VS Code's command line, or VSCodium's, which runs `serve-web`. Blocking: it may
//! run the login shell for its `PATH`, so run it off the UI thread.
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// An editor whose command line runs `serve-web`: VS Code, or a fork that
/// ships the same command line and hosts the server build it downloads.
/// Others are not looked for: a fork with the command but no hosted build
/// never finishes its first start.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Product {
    name: &'static str,
    /// The command it puts on `PATH`.
    command: &'static str,
    /// The program beside the command that serves. On macOS and Windows the
    /// command is a launcher script that runs the editor's Electron as Node,
    /// which runs this program in turn; started directly, it is the app's
    /// own child, so stopping that child stops the server, with no launcher
    /// left in between.
    server: &'static str,
    /// Where it keeps its command when it was never put on `PATH`.
    bundled: &'static [&'static str],
    /// Whether its server is under license terms the user must accept
    /// before the app starts it. VSCodium's server is MIT, with none.
    licensed: bool,
}

impl Product {
    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    pub(crate) fn licensed(&self) -> bool {
        self.licensed
    }
}

pub(crate) const VS_CODE: Product = Product {
    name: "VS Code",
    command: if cfg!(windows) { "code.cmd" } else { "code" },
    server: if cfg!(windows) {
        "code-tunnel.exe"
    } else {
        "code-tunnel"
    },
    bundled: if cfg!(target_os = "macos") {
        &["/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"]
    } else {
        &[]
    },
    licensed: true,
};

pub(crate) const VSCODIUM: Product = Product {
    name: "VSCodium",
    command: if cfg!(windows) {
        "codium.cmd"
    } else {
        "codium"
    },
    server: if cfg!(windows) {
        "codium-tunnel.exe"
    } else {
        "codium-tunnel"
    },
    bundled: if cfg!(target_os = "macos") {
        &["/Applications/VSCodium.app/Contents/Resources/app/bin/codium"]
    } else {
        &[]
    },
    licensed: false,
};

/// The editors looked for, in order: the first one installed is used, so VS
/// Code wins over VSCodium wherever each is found.
pub(crate) const PRODUCTS: &[Product] = &[VS_CODE, VSCODIUM];

/// The editors looked for, as a message names them: "VS Code or VSCodium".
pub(crate) fn product_names() -> String {
    PRODUCTS
        .iter()
        .map(Product::name)
        .collect::<Vec<_>>()
        .join(" or ")
}

/// An installed editor and the program of it that runs `serve-web`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Install {
    product: &'static Product,
    program: PathBuf,
}

impl Install {
    pub(crate) fn new(product: &'static Product, program: impl Into<PathBuf>) -> Self {
        Self {
            product,
            program: program.into(),
        }
    }

    pub(crate) fn product(&self) -> &'static Product {
        self.product
    }

    pub(crate) fn program(&self) -> &Path {
        &self.program
    }
}

/// The first editor installed, with its program that runs `serve-web`.
pub(super) fn find() -> Option<Install> {
    find_in(
        crate::login_env::path().as_deref(),
        PRODUCTS.iter().map(|product| (product, product.bundled)),
    )
}

/// For each product in turn, its command on `path`, else the first of its
/// bundled commands there is: the first found, as the program that serves.
fn find_in<B>(
    path: Option<&OsStr>,
    products: impl IntoIterator<Item = (&'static Product, B)>,
) -> Option<Install>
where
    B: IntoIterator,
    B::Item: AsRef<Path>,
{
    let dirs: Vec<PathBuf> = path
        .into_iter()
        .flat_map(std::env::split_paths)
        .filter(|dir| dir.is_absolute())
        .collect();
    products.into_iter().find_map(|(product, bundled)| {
        dirs.iter()
            .map(|dir| dir.join(product.command))
            .chain(bundled.into_iter().map(|path| path.as_ref().to_path_buf()))
            .find(|command| runnable(command))
            .map(|command| Install::new(product, server(&command, product.server)))
    })
}

/// The `server` program beside `command`, through any links to it, else
/// `command` itself: VS Code's standalone command line serves on its own.
fn server(command: &Path, server: &str) -> PathBuf {
    let real = std::fs::canonicalize(command).unwrap_or_else(|_| command.to_owned());
    real.parent()
        .map(|dir| dir.join(server))
        .filter(|server| runnable(server))
        .unwrap_or(real)
}

fn runnable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(windows)]
    metadata.is_file()
}

#[cfg(test)]
mod tests;
