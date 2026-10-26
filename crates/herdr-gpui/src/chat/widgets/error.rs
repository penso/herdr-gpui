#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    // keep-sorted start
    #[error("invalid interactive card: action is no longer present")]
    ActionGone,
    #[error("invalid interactive card: component fields or bounds")]
    Component,
    #[error("invalid interactive card: component count")]
    ComponentCount,
    #[error("invalid interactive card: field ids must be unique ASCII names")]
    FieldId,
    #[error("invalid interactive card: form value")]
    FormValue,
    #[error("invalid interactive card JSON")]
    Json(#[from] serde_json::Error),
    #[error("invalid interactive card: title")]
    Title,
    #[error("the interactive card exceeds its size limit")]
    TooLarge,
    #[error("invalid interactive card: too many editable fields")]
    TooManyFields,
    // keep-sorted end
}
