//! Reading a recipe's files from the host its project is on, through the
//! script runner Teleport uses, so a project on an SSH host or a cloud
//! machine builds as one on this computer does. Only files Git would commit
//! are read (tracked, or untracked and not ignored), never a symbolic link,
//! and never from a folder that resolves outside the project, so a link or
//! an ignored secret beside the recipe cannot leave the host.

use super::{Error, Field, Result};
use herdr_client::shell_quote;
use std::sync::atomic::AtomicBool;

/// The most files one build context may hold.
const MAX_FILES: usize = 10_000;

/// One file from the project, its path relative to where it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub executable: bool,
    pub bytes: Vec<u8>,
}

/// The script printing every file under `dir` (from the project root
/// `root`) that a Git pathspec in `pathspecs` matches, framed as
/// `path NUL mode NUL size NUL bytes`.
/// A `dir` whose physical path leaves the project, through a linked folder,
/// fails rather than listing another tree's files. Below `dir`, a file under
/// a linked folder is skipped: Git's index can still name `conf/a` after
/// `conf` became a link, and opening it would follow the link.
pub fn script(root: &str, dir: &str, pathspecs: &[String]) -> String {
    let pathspecs = pathspecs
        .iter()
        .map(|pathspec| shell_quote(pathspec))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"cd {root} || exit 3
top=$(pwd -P)
cd {dir} || exit 0
case $(pwd -P) in "$top" | "$top"/*) ;; *) exit 4 ;; esac
git ls-files -z --cached --others --exclude-standard -- {pathspecs} | xargs -0 sh -c '
for f; do
  [ -f "$f" ] && [ ! -L "$f" ] || continue
  p=$f linked=
  while case $p in */*) true ;; *) false ;; esac; do
    p=${{p%/*}}
    if [ -L "$p" ]; then linked=1; break; fi
  done
  [ -z "$linked" ] || continue
  if [ -x "$f" ]; then m=x; else m=-; fi
  s=$(wc -c < "$f" | tr -d " ")
  printf "%s\0%s\0%s\0" "$f" "$m" "$s"
  head -c "$s" "$f"
done' sh
"#,
        root = shell_quote(root),
        dir = shell_quote(dir),
    )
}

/// The files `output` frames, as [`script`] prints them.
pub fn parse(mut output: &[u8]) -> Result<Vec<File>> {
    let mut files = Vec::new();
    while !output.is_empty() {
        if files.len() == MAX_FILES {
            return Err(Error::TooLarge);
        }
        let mut field = || -> Result<String> {
            let end = output
                .iter()
                .position(|&b| b == 0)
                .ok_or(Error::Truncated)?;
            let text = std::str::from_utf8(&output[..end])
                .map_err(|_| Error::Field(Field::FileName))?
                .to_owned();
            output = &output[end + 1..];
            Ok(text)
        };
        let path = field()?;
        let executable = field()? == "x";
        let size: usize = field()?.parse().map_err(|_| Error::Truncated)?;
        if output.len() < size {
            return Err(Error::Truncated);
        }
        let (bytes, rest) = output.split_at(size);
        output = rest;
        if path.is_empty() || path.starts_with('/') || path.split('/').any(|part| part == "..") {
            return Err(Error::Field(Field::ContextPath));
        }
        files.push(File {
            path,
            executable,
            bytes: bytes.to_vec(),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Read the files `pathspecs` match under `dir` in the project at `root` on
/// `host`. Blocks: run it on a worker.
pub fn read(
    host: &crate::teleport::Host,
    root: &str,
    dir: &str,
    pathspecs: &[String],
    cancelled: &AtomicBool,
) -> Result<Vec<File>> {
    if pathspecs.is_empty() {
        return Ok(Vec::new());
    }
    let output = host
        .capture(&script(root, dir, pathspecs), cancelled)
        .map_err(Error::Host)?;
    parse(&output)
}

/// The one file at `path` in the project at `root`, if it is there.
pub fn read_one(
    host: &crate::teleport::Host,
    root: &str,
    path: &str,
    cancelled: &AtomicBool,
) -> Result<Option<File>> {
    let (dir, name) = path.rsplit_once('/').unwrap_or((".", path));
    let found = read(host, root, dir, &[format!(":(literal){name}")], cancelled)?;
    Ok(found.into_iter().find(|file| file.path == name))
}

#[cfg(test)]
mod tests;
