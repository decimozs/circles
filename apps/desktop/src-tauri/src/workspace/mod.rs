pub mod copy;
pub mod events;
pub mod model;
pub mod names;
pub mod service;
pub mod state;

pub use events::{EventSink, Locks, TauriSink};
pub use model::{
    BranchInfo, BranchState, CreateOptions, PendingPrompt, PullRequest, ReconcileReport, Section,
    Workspace, WorkspaceEvent,
};
pub use service::WorkspaceService;
