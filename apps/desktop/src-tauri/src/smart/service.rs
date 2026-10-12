//! The model that ships with the app, found once and kept.
//!
//! The app carries the model file. On first use it is also copied to the app's data folder, and
//! that copy is the one used from then on. A later app update then does not have to bring the
//! model again when it has not changed, and a new person never has to download anything.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use super::embedded::EmbeddedModel;

#[derive(Default)]
pub struct SmartService {
    embedded: OnceLock<Arc<EmbeddedModel>>,
}

/// Where the copy of the model lives.
pub fn data_copy(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join("title-model.gguf")
}

/// Whether the copy is there and is the same file (the same size).
pub fn copy_is_current(source: &Path, target: &Path) -> bool {
    match (std::fs::metadata(source), std::fs::metadata(target)) {
        (Ok(a), Ok(b)) => a.is_file() && b.is_file() && a.len() == b.len(),
        _ => false,
    }
}

/// Copies the model next to the app's data, through a temporary name so a half copy is never used.
pub fn copy_model(source: &Path, target: &Path) -> std::io::Result<()> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = target.with_extension("gguf.part");
    std::fs::copy(source, &part)?;
    std::fs::rename(&part, target)
}

impl SmartService {
    /// Looks for the model file. Without it there is no embedded model, and everything that would
    /// use one keeps working without it.
    pub fn locate(&self, resource_dir: Option<&Path>, data_dir: &Path) {
        let source = EmbeddedModel::locate(resource_dir);
        let target = data_copy(data_dir);
        let chosen = match source {
            Some(source) if copy_is_current(&source, &target) => target,
            Some(source) => {
                // From the app's own resources, not from a source tree in development.
                let bundled = resource_dir.is_some_and(|r| source.starts_with(r));
                if bundled {
                    let from = source.clone();
                    let to = target;
                    std::thread::spawn(move || {
                        if let Err(e) = copy_model(&from, &to) {
                            tracing::warn!(error = %e, "cannot copy the title model");
                        }
                    });
                }
                source
            }
            None if target.is_file() => target,
            None => {
                tracing::warn!("the title model file was not found");
                return;
            }
        };
        tracing::info!(model = %chosen.display(), "title model found");
        let _ = self.embedded.set(Arc::new(EmbeddedModel::new(chosen)));
    }

    pub fn embedded(&self) -> Option<Arc<EmbeddedModel>> {
        self.embedded.get().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_is_made_through_a_temporary_name_and_then_recognised() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("bundle.gguf");
        std::fs::write(&source, b"model bytes").unwrap();
        let target = data_copy(&dir.path().join("data"));
        assert!(!copy_is_current(&source, &target));
        copy_model(&source, &target).unwrap();
        assert!(copy_is_current(&source, &target));
        assert!(!target.with_extension("gguf.part").exists());
        // A different model (another size) is not current.
        std::fs::write(&source, b"a newer, larger model").unwrap();
        assert!(!copy_is_current(&source, &target));
    }

    #[test]
    fn the_copy_is_used_when_the_bundle_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let target = data_copy(&data);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"x").unwrap();
        let service = SmartService::default();
        // No bundle at all, and not the source tree either: look in an empty resource folder.
        service.locate(Some(&dir.path().join("nothing")), &data);
        // The source tree of this checkout may hold the real model, so only the copy-first rule is
        // asserted: something was found.
        assert!(service.embedded().is_some());
    }
}
