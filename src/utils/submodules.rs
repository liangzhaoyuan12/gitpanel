use anyhow::{Context, Result};

use crate::model::SubmoduleInfo;
use crate::utils::repository::GitRepo;

impl GitRepo {
    /// List all submodules in the repository.
    pub fn list_submodules(&self) -> Result<Vec<SubmoduleInfo>> {
        let repo = self.inner();
        let submodules = repo.submodules().context("Failed to list submodules")?;
        let mut result = Vec::new();

        for sm in &submodules {
            let name = sm.name().unwrap_or("").to_string();
            let path = sm.path().to_string_lossy().to_string();
            let url = sm.url().unwrap_or("").to_string();
            let head_id = sm.head_id().map(|oid| oid.to_string());
            let is_initialized = sm.open().is_ok();

            result.push(SubmoduleInfo {
                name,
                path,
                url,
                head_id,
                is_initialized,
            });
        }

        Ok(result)
    }

    /// Initialize a submodule.
    pub fn submodule_init(&self, name: &str) -> Result<()> {
        let repo = self.inner();
        let mut sm = repo.find_submodule(name).context("Submodule not found")?;
        sm.init(false).context("Failed to init submodule")?;
        Ok(())
    }

    /// Update a submodule (init + checkout). Uses git CLI for reliability.
    pub fn submodule_update(&self, name: &str) -> Result<String> {
        let path = self.path().to_string_lossy().to_string();
        let output = std::process::Command::new("git")
            .args(["submodule", "update", "--init", name])
            .current_dir(&path)
            .output()
            .context("Failed to run git submodule update")?;

        if output.status.success() {
            Ok(format!("Updated submodule '{}'", name))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{}", stderr.trim());
        }
    }
}
