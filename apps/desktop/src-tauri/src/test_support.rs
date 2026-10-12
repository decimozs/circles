//! Small builders shared by unit tests.

use crate::project::Project;
use crate::workspace::{state::WorkspaceState, Workspace};

pub fn project_fixture(name: &str, path: &str) -> Project {
    Project {
        id: "p1".into(),
        name: name.into(),
        path: path.into(),
        path_exists: true,
        default_branch: "main".into(),
        remote_url: None,
        avatar_url: None,
        workspace_count: 0,
        created_at: 0,
        updated_at: 0,
    }
}

pub fn workspace_fixture(city: &str, path: &str, base: &str) -> Workspace {
    Workspace {
        id: "w1".into(),
        project_id: "p1".into(),
        city: city.into(),
        title: None,
        branch: "me/abc12".into(),
        base_branch: base.into(),
        path: path.into(),
        state: WorkspaceState::Ready,
        error: None,
        pinned: false,
        section_id: None,
        pr_number: None,
        pr_url: None,
        created_at: 0,
        updated_at: 0,
        last_opened_at: None,
        archived_at: None,
        pending_prompt: None,
        pending_model: None,
        pending_effort: None,
    }
}

use std::sync::Mutex;

use crate::pty::{ExitInfo, PtySink};

/// A terminal sink that remembers what it was sent.
#[derive(Default)]
pub struct CollectPtySink {
    chunks: Mutex<Vec<Vec<u8>>>,
    exit: Mutex<Option<ExitInfo>>,
    resets: Mutex<usize>,
}

impl CollectPtySink {
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::default())
    }

    pub fn text(&self) -> String {
        let bytes: Vec<u8> = self
            .chunks
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .copied()
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    pub fn exit(&self) -> Option<ExitInfo> {
        self.exit.lock().unwrap().clone()
    }

    pub fn resets(&self) -> usize {
        *self.resets.lock().unwrap()
    }
}

impl PtySink for CollectPtySink {
    fn data(&self, bytes: Vec<u8>) {
        self.chunks.lock().unwrap().push(bytes);
    }

    fn exit(&self, info: &ExitInfo) {
        *self.exit.lock().unwrap() = Some(info.clone());
    }

    fn reset(&self) {
        // A reset clears the view, so what came before no longer counts.
        self.chunks.lock().unwrap().clear();
        *self.exit.lock().unwrap() = None;
        *self.resets.lock().unwrap() += 1;
    }
}
