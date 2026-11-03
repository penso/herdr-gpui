use super::*;
use crate::cloud::{
    CloudProvider,
    image::{
        Project,
        catalog::{Entry, Image as Built, Origin, Outcome},
    },
};

#[test]
fn images_say_what_new_machines_start_from_or_why_not() {
    let entry = Entry {
        project: Project {
            id: "p".into(),
            label: "app".into(),
        },
        origin: Origin {
            endpoint_id: "local".into(),
            path: "/src/app".into(),
        },
        images: vec![
            Built {
                provider: CloudProvider::Container,
                key: "k1".into(),
                finished_at: 5,
                outcome: Outcome::Ready {
                    reference: "herdr-app-p:k1".into(),
                },
                stale: Vec::new(),
            },
            Built {
                provider: CloudProvider::Daytona,
                key: "k2".into(),
                finished_at: 6,
                outcome: Outcome::Failed {
                    log: vec!["boom".into()],
                },
                stale: Vec::new(),
            },
        ],
    };
    let images = images_of(&[entry]);
    assert_eq!(images.len(), 2);
    assert_eq!(images[0].state, "ready");
    assert_eq!(images[1].reference, None);
    let text = images_text(&images);
    assert!(
        text.contains("app on Container: ready, herdr-app-p:k1"),
        "{text}"
    );
    assert!(
        text.contains("app on Daytona: the last build failed"),
        "{text}"
    );
    assert!(images_text(&[]).contains("cloud images build"));
}
