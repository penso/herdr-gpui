#![allow(clippy::unwrap_used)]
use super::*;
use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard, PoisonError},
};

/// A fresh catalog for one test; tests holding one run one at a time.
pub struct Isolated {
    dir: tempfile::TempDir,
    _serial: MutexGuard<'static, ()>,
}

impl Isolated {
    pub fn file(&self) -> PathBuf {
        self.dir.path().join(FILE)
    }
}

impl Drop for Isolated {
    fn drop(&mut self) {
        *TEST_PATH.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

pub fn isolated() -> Isolated {
    static SERIAL: Mutex<()> = Mutex::new(());
    let serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    *TEST_PATH.lock().unwrap_or_else(PoisonError::into_inner) = Some(dir.path().join(FILE));
    Isolated {
        dir,
        _serial: serial,
    }
}

fn project(id: &str) -> Project {
    Project {
        id: id.into(),
        label: "app".into(),
    }
}

fn ready(provider: CloudProvider, key: &str) -> Image {
    Image {
        provider,
        key: key.into(),
        finished_at: 1,
        outcome: Outcome::Ready {
            reference: format!("img:{key}"),
        },
        stale: Vec::new(),
    }
}

fn origin() -> Origin {
    Origin {
        endpoint_id: "local".into(),
        path: "/src/app".into(),
    }
}

#[test]
fn one_image_per_project_and_provider_is_kept() {
    let _dir = isolated();
    assert!(load().unwrap().is_empty());
    assert_eq!(
        record(
            &project("p"),
            origin(),
            ready(CloudProvider::Container, "k1")
        )
        .unwrap(),
        Vec::<String>::new()
    );
    let stale = record(
        &project("p"),
        origin(),
        ready(CloudProvider::Container, "k2"),
    )
    .unwrap();
    assert_eq!(stale, ["img:k1"]);
    record(&project("p"), origin(), ready(CloudProvider::Daytona, "k3")).unwrap();
    record(
        &project("q"),
        origin(),
        ready(CloudProvider::Container, "k4"),
    )
    .unwrap();
    let projects = load().unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].images.len(), 2);
    assert_eq!(projects[0].images[0].key, "k2");
    assert_eq!(projects[0].images[0].stale, ["img:k1"]);
    deleted("p", CloudProvider::Container, &["img:k1".into()]).unwrap();
    assert!(load().unwrap()[0].images[0].stale.is_empty());
}

#[test]
fn a_replaced_image_is_kept_until_it_is_deleted() {
    let _dir = isolated();
    let container = CloudProvider::Container;
    record(&project("p"), origin(), ready(container, "k1")).unwrap();
    // A failed rebuild still remembers the ready image it replaced.
    let failed = Image {
        outcome: Outcome::Failed { log: Vec::new() },
        ..ready(container, "k2")
    };
    assert_eq!(record(&project("p"), origin(), failed).unwrap(), ["img:k1"]);
    // Rebuilt from the first recipe: its image is current, not stale.
    assert!(
        record(&project("p"), origin(), ready(container, "k1"))
            .unwrap()
            .is_empty()
    );
    // Deletes keep failing: the list stays bounded.
    for key in 0..MAX_STALE + 4 {
        record(&project("p"), origin(), ready(container, &key.to_string())).unwrap();
    }
    let stale = &load().unwrap()[0].images[0].stale;
    assert_eq!(stale.len(), MAX_STALE);
    assert_eq!(stale.last().unwrap(), &format!("img:{}", MAX_STALE + 2));
}

#[test]
fn forgetting_the_last_image_forgets_the_project() {
    let _dir = isolated();
    record(
        &project("p"),
        origin(),
        ready(CloudProvider::Container, "k"),
    )
    .unwrap();
    assert_eq!(forget("p", CloudProvider::Daytona).unwrap(), None);
    assert_eq!(
        forget("p", CloudProvider::Container).unwrap(),
        Some(ready(CloudProvider::Container, "k"))
    );
    assert!(load().unwrap().is_empty());
    assert_eq!(forget("missing", CloudProvider::Container).unwrap(), None);
}

#[test]
fn a_failed_build_keeps_its_log_and_survives_a_reload() {
    let _dir = isolated();
    let failed = Image {
        outcome: Outcome::Failed {
            log: vec!["step 1".into(), "boom".into()],
        },
        ..ready(CloudProvider::Container, "k")
    };
    record(&project("p"), origin(), failed.clone()).unwrap();
    assert_eq!(load().unwrap()[0].images, [failed]);
}

#[test]
fn a_corrupt_catalog_is_an_error_not_an_empty_one() {
    let catalog = isolated();
    fs::write(catalog.file(), "{").unwrap();
    assert!(matches!(load(), Err(Error::Json(_))));
    fs::write(catalog.file(), r#"{"version": 9, "projects": []}"#).unwrap();
    assert!(matches!(load(), Err(Error::Field(Field::Catalog))));
}
