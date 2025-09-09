//! Atom IDE Legacy Atom Package Compatibility Layer
//!
//! Provides compatibility with legacy Atom packages through Node.js bridge

use atom_core::BufferManager;
use atom_ipc::{CoreRequest, CoreResponse};
use atom_settings::Settings;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;
use tokio::process::Command as AsyncCommand;
use tracing::{error, info, warn};

#[derive(Debug, Error)]
pub enum AtomCompatError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Node.js not found")]
    NodeNotFound,
    #[error("Package installation failed: {0}")]
    InstallationFailed(String),
    #[error("CoffeeScript transpilation failed: {0}")]
    TranspilationFailed(String),
}

/// Atom package metadata
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AtomPackage {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub main: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub repository: Option<String>,
    pub dependencies: Option<HashMap<String, String>>,
    pub engines: Option<HashMap<String, String>>,
}

/// Legacy Atom compatibility bridge
pub struct AtomCompatBridge {
    settings: Settings,
    installed_packages: HashMap<String, AtomPackage>,
    package_paths: HashMap<String, PathBuf>,
    node_path: Option<String>,
}

impl AtomCompatBridge {
    /// Create new Atom compatibility bridge
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            installed_packages: HashMap::new(),
            package_paths: HashMap::new(),
            node_path: None,
        }
    }

    /// Initialize the bridge and detect Node.js
    pub async fn initialize(&mut self) -> Result<(), AtomCompatError> {
        // Detect Node.js installation
        self.node_path = Some(self.detect_node().await?);
        info!("Node.js detected at: {}", self.node_path.as_ref().unwrap());

        // Load installed packages
        self.load_installed_packages().await?;

        Ok(())
    }

    /// Detect Node.js installation
    async fn detect_node(&self) -> Result<String, AtomCompatError> {
        let candidates = vec![
            "node",
            "nodejs",
            "/usr/bin/node",
            "/usr/local/bin/node",
            "C:\\Program Files\\nodejs\\node.exe",
            "C:\\Program Files (x86)\\nodejs\\node.exe",
        ];

        for candidate in candidates {
            let output = AsyncCommand::new(candidate).arg("--version").output().await;

            if let Ok(output) = output {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout);
                    info!("Found Node.js {} at {}", version.trim(), candidate);
                    return Ok(candidate.to_string());
                }
            }
        }

        Err(AtomCompatError::NodeNotFound)
    }

    /// Load information about installed Atom packages
    async fn load_installed_packages(&mut self) -> Result<(), AtomCompatError> {
        // Scan common Atom package directories
        let package_dirs = vec![
            PathBuf::from(".atom/packages"),
            PathBuf::from("~/.atom/packages")
                .expand_user()
                .unwrap_or_default(),
        ];

        for dir in package_dirs {
            if dir.exists() {
                self.scan_package_directory(&dir).await?;
            }
        }

        info!("Loaded {} Atom packages", self.installed_packages.len());
        Ok(())
    }

    /// Scan directory for Atom packages
    async fn scan_package_directory(&mut self, dir: &Path) -> Result<(), AtomCompatError> {
        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let package_json = path.join("package.json");
                if package_json.exists() {
                    match self.load_package_manifest(&package_json).await {
                        Ok(package) => {
                            let name = package.name.clone();
                            self.installed_packages.insert(name.clone(), package);
                            self.package_paths.insert(name, path);
                        }
                        Err(e) => {
                            warn!("Failed to load package at {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Load package manifest (package.json)
    async fn load_package_manifest(&self, path: &Path) -> Result<AtomPackage, AtomCompatError> {
        let content = tokio::fs::read_to_string(path).await?;
        let package: AtomPackage = serde_json::from_str(&content)?;
        Ok(package)
    }

    /// Transpile CoffeeScript to JavaScript using Node.js
    pub async fn transpile_coffeescript(&self, source: &str) -> Result<String, AtomCompatError> {
        let node_path = self
            .node_path
            .as_ref()
            .ok_or(AtomCompatError::NodeNotFound)?;

        // Create temporary CoffeeScript transpiler script
        let transpiler_script = r#"
const fs = require('fs');
const coffeescript = require('coffeescript');

const source = fs.readFileSync(process.argv[2], 'utf8');
try {
    const compiled = coffeescript.compile(source, {
        header: false,
        sourceMap: false
    });
    console.log(compiled);
} catch (error) {
    console.error('CoffeeScript compilation error:', error.message);
    process.exit(1);
}
"#;

        // Write source to temporary file
        let temp_source = std::env::temp_dir().join("atom_compat_source.coffee");
        tokio::fs::write(&temp_source, source).await?;

        // Write transpiler script
        let temp_script = std::env::temp_dir().join("transpiler.js");
        tokio::fs::write(&temp_script, transpiler_script).await?;

        // Execute transpilation
        let output = AsyncCommand::new(node_path)
            .arg(temp_script.to_str().unwrap())
            .arg(temp_source.to_str().unwrap())
            .output()
            .await?;

        // Clean up temporary files
        let _ = tokio::fs::remove_file(&temp_source).await;
        let _ = tokio::fs::remove_file(&temp_script).await;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(AtomCompatError::TranspilationFailed(error.to_string()))
        }
    }

    /// Install Atom package from GitHub
    pub async fn install_package(&mut self, package_spec: &str) -> Result<(), AtomCompatError> {
        // Parse package specification (e.g., "username/package-name")
        let parts: Vec<&str> = package_spec.split('/').collect();
        if parts.len() != 2 {
            return Err(AtomCompatError::InstallationFailed(
                "Invalid package specification. Use 'username/package-name'".to_string(),
            ));
        }

        let (username, package_name) = (parts[0], parts[1]);
        let github_url = format!("https://github.com/{}/{}", username, package_name);

        info!("Installing Atom package: {}", package_spec);

        // Clone or download package from GitHub
        let package_dir = PathBuf::from(".atom/packages").join(package_name);

        if package_dir.exists() {
            warn!("Package {} already exists, skipping", package_name);
            return Ok(());
        }

        // Create packages directory
        tokio::fs::create_dir_all(package_dir.parent().unwrap()).await?;

        // Use git to clone if available, otherwise download zip
        let git_result = AsyncCommand::new("git")
            .args(&["clone", &github_url, package_dir.to_str().unwrap()])
            .status()
            .await;

        match git_result {
            Ok(status) if status.success() => {
                info!("Successfully cloned package {}", package_spec);
                // Reload package information
                self.load_installed_packages().await?;
                Ok(())
            }
            _ => {
                error!("Failed to install package {} via git", package_spec);
                Err(AtomCompatError::InstallationFailed(format!(
                    "Git clone failed for {}. Please install git or manually download the package.",
                    package_spec
                )))
            }
        }
    }

    /// Get list of installed packages
    pub fn list_packages(&self) -> Vec<&AtomPackage> {
        self.installed_packages.values().collect()
    }

    /// Check if package is installed
    pub fn is_package_installed(&self, name: &str) -> bool {
        self.installed_packages.contains_key(name)
    }

    /// Get package path
    pub fn get_package_path(&self, name: &str) -> Option<&PathBuf> {
        self.package_paths.get(name)
    }
}

// Utility trait for expanding user home directory in paths
trait PathExpansion {
    fn expand_user(&self) -> Option<PathBuf>;
}

impl PathExpansion for PathBuf {
    fn expand_user(&self) -> Option<PathBuf> {
        if let Some(path_str) = self.to_str() {
            if path_str.starts_with("~") {
                if let Some(home) = dirs::home_dir() {
                    return Some(home.join(&path_str[2..]));
                }
            }
        }
        Some(self.clone())
    }
}
