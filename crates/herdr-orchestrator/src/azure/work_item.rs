//! Azure Boards work items in the shared issue shape. A WIQL query names the
//! project's open work items, most recently changed first, and a batch read
//! fetches their fields.

use super::Identity;
use crate::{Activity, Item, ItemKey, SourceKey};
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// The most work items one sync reads. A project with more is shown but not
/// cached, since a capped listing is no full sync.
pub const MAX_OPEN: usize = 200;

/// Every work item of the repository's project that is not finished. The
/// states are the closing ones of the Agile, Scrum, CMMI, and Basic processes.
pub const QUERY: &str = "SELECT [System.Id] FROM WorkItems \
     WHERE [System.TeamProject] = @project \
     AND [System.State] NOT IN ('Closed', 'Done', 'Removed', 'Resolved', 'Completed', 'Cut') \
     ORDER BY [System.ChangedDate] DESC";

/// The fields the batch read asks for, and no more.
pub const FIELDS: &str = "System.Id,System.Title,System.State,System.WorkItemType,\
System.CreatedDate,System.ChangedDate,System.CreatedBy,System.Tags,System.CommentCount,\
System.Description,System.Parent";

/// The reply of `POST _apis/wit/wiql`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryReply {
    #[serde(default)]
    pub work_items: Vec<Reference>,
}

#[derive(Deserialize)]
pub struct Reference {
    pub id: u64,
}

/// The reply of `GET _apis/wit/workitems?ids=`.
#[derive(Deserialize)]
pub struct Batch {
    pub value: Vec<WorkItemNode>,
}

#[derive(Deserialize)]
pub struct WorkItemNode {
    id: u64,
    fields: Fields,
}

#[derive(Deserialize)]
struct Fields {
    #[serde(rename = "System.Title")]
    title: String,
    #[serde(rename = "System.State")]
    state: String,
    #[serde(rename = "System.WorkItemType")]
    kind: Option<String>,
    #[serde(rename = "System.CreatedDate")]
    created: Option<DateTime<Utc>>,
    #[serde(rename = "System.ChangedDate")]
    changed: Option<DateTime<Utc>>,
    #[serde(rename = "System.CreatedBy")]
    created_by: Option<Identity>,
    /// Semicolon-separated.
    #[serde(rename = "System.Tags")]
    tags: Option<String>,
    #[serde(rename = "System.CommentCount")]
    comments: Option<u64>,
    /// HTML, shown as the item's description.
    #[serde(rename = "System.Description")]
    description: Option<String>,
    #[serde(rename = "System.Parent")]
    parent: Option<u64>,
}

impl WorkItemNode {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// `web` is the project's address, which work item links hang from.
    pub fn into_item(self, source: &SourceKey, web: &str) -> Item {
        let fields = self.fields;
        // The process's own state (`Active`, `In Progress`) and the kind
        // (`Bug`, `Task`) lead the labels, as Azure Boards shows them.
        let labels = Some(fields.state)
            .filter(|state| !state.is_empty())
            .into_iter()
            .chain(fields.kind)
            .chain(
                fields
                    .tags
                    .iter()
                    .flat_map(|tags| tags.split(';'))
                    .map(str::trim)
                    .filter(|tag| !tag.is_empty())
                    .map(str::to_owned),
            )
            .collect();
        Item {
            key: ItemKey {
                source: source.clone(),
                native_id: self.id.to_string(),
            },
            identifier: format!("#{}", self.id),
            title: fields.title,
            description: fields.description.filter(|body| !body.is_empty()),
            // Every listed item is unfinished; its own state is a label.
            state: "open".to_owned(),
            url: Some(format!("{web}/_workitems/edit/{}", self.id)),
            author: fields.created_by.and_then(Identity::name),
            labels,
            parent_id: fields.parent.map(|parent| parent.to_string()),
            blocked_by: Vec::new(),
            priority: None,
            created_at: fields.created,
            updated_at: fields.changed.or(fields.created),
            pull_request: None,
            activity: Some(Activity {
                comments: fields.comments,
                ..Activity::default()
            }),
        }
    }
}
