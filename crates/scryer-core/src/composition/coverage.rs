//! Checking the model's source map against the real project.

use crate::composition::codebase::manifest_dirs;
use crate::domain::validate;
use crate::ScryModel;
use std::path::Path;

/// Manifest directories the model covers nowhere, and source directories two
/// containers both claim.
pub fn validate_coverage(model: &ScryModel, project_path: &Path) -> Vec<String> {
    validate::validate_coverage_with(model, project_path, &manifest_dirs(project_path))
}
