//! Typed requests that reorder the daemon's workspace list.
//!
//! Herdr offers two reorders. `workspace.move` takes one workspace to an
//! index in the whole list, counted with the workspace still in place, and
//! announces `workspace.moved`. `workspace.move_block` lifts several
//! workspaces out, wherever they sit, and lands them together before another
//! one or at the end, announcing `workspace.reordered`. Field names match
//! Herdr's `api::schema::workspaces`.

use crate::{ClientHandle, Result, method::Method};
use serde::{Deserialize, Serialize};

/// `workspace.move` parameters. The daemon removes the workspace and inserts
/// it at `insert_index`, less one when that lies past where it was, so an
/// index of `len` sends it last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMoveParams {
    pub workspace_id: String,
    pub insert_index: usize,
}

/// `workspace.move_block` parameters: `workspace_ids` land, in that order,
/// before `before_workspace_id`, or at the end of the list without one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMoveBlockParams {
    pub workspace_ids: Vec<String>,
    #[serde(default)]
    pub before_workspace_id: Option<String>,
}

/// One reorder of the workspace list, as whichever method carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceReorder {
    Move(WorkspaceMoveParams),
    MoveBlock(WorkspaceMoveBlockParams),
}

impl WorkspaceReorder {
    pub fn method(&self) -> Method {
        match self {
            Self::Move(_) => Method::WorkspaceMove,
            Self::MoveBlock(_) => Method::WorkspaceMoveBlock,
        }
    }

    pub fn params(&self) -> Result<serde_json::Value> {
        Ok(match self {
            Self::Move(params) => serde_json::to_value(params)?,
            Self::MoveBlock(params) => serde_json::to_value(params)?,
        })
    }
}

impl ClientHandle {
    /// Queues a workspace reorder, returning the request ID its response
    /// carries.
    pub fn reorder_workspaces(&self, boot_id: &str, reorder: &WorkspaceReorder) -> Result<String> {
        self.request(boot_id, reorder.method(), reorder.params()?)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn move_serializes_herdr_s_field_names() {
        let reorder = WorkspaceReorder::Move(WorkspaceMoveParams {
            workspace_id: "w2".into(),
            insert_index: 0,
        });
        assert_eq!(reorder.method(), Method::WorkspaceMove);
        assert_eq!(
            reorder.params().unwrap(),
            json!({"workspace_id": "w2", "insert_index": 0})
        );
    }

    #[test]
    fn move_block_keeps_an_explicit_end_anchor() {
        let reorder = WorkspaceReorder::MoveBlock(WorkspaceMoveBlockParams {
            workspace_ids: vec!["a".into(), "a1".into()],
            before_workspace_id: None,
        });
        assert_eq!(reorder.method(), Method::WorkspaceMoveBlock);
        assert_eq!(
            reorder.params().unwrap(),
            json!({"workspace_ids": ["a", "a1"], "before_workspace_id": null})
        );
        // Herdr defaults a missing anchor, so both spellings mean the end.
        let parsed: WorkspaceMoveBlockParams =
            serde_json::from_value(json!({"workspace_ids": ["a"]})).unwrap();
        assert_eq!(parsed.before_workspace_id, None);
    }
}
