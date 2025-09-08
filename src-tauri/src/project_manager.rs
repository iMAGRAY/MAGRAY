use anyhow::Result;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use tokio::sync::{RwLock, mpsc, Mutex};
use tokio::time::timeout;
use uuid::Uuid;
use walkdir::WalkDir;
use notify::{Watcher, RecursiveMode, Event, RecommendedWatcher};
use notify::EventKind;
use tracing::{info, warn, error, debug};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Unique identifier for a project
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(Uuid);

impl ProjectId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
    
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

/// Project configuration with validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub project_type: ProjectType,
    pub root_path: PathBuf,
    pub ignore_patterns: Vec<String>,
    pub include_patterns: Vec<String>,
    pub build_command: Option<String>,
    pub test_command: Option<String>,
    pub language_servers: Vec<LanguageServerConfig>,
}

impl ProjectConfig {
    /// Validate configuration for security and correctness
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(anyhow::anyhow!("Project name cannot be empty"));
        }
        
        if self.name.len() > 255 {
            return Err(anyhow::anyhow!("Project name too long (max 255 characters)"));
        }
        
        // Validate name contains only safe characters
        if !self.name.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.')) {
            return Err(anyhow::anyhow!("Project name contains invalid characters"));
        }
        
        if !self.root_path.is_absolute() {
            return Err(anyhow::anyhow!("Root path must be absolute"));
        }
        
        // Validate ignore patterns for security
        for pattern in &self.ignore_patterns {
            if pattern.contains("..") {
                return Err(anyhow::anyhow!("Invalid ignore pattern with directory traversal: {}", pattern));
            }
        }
        
        // Validate commands don't contain dangerous patterns
        if let Some(ref cmd) = self.build_command {
            if cmd.contains("rm -rf") || cmd.contains("del /f") || cmd.contains("format") {
                return Err(anyhow::anyhow!("Potentially dangerous build command detected"));
            }
        }
        
        if let Some(ref cmd) = self.test_command {
            if cmd.contains("rm -rf") || cmd.contains("del /f") || cmd.contains("format") {
                return Err(anyhow::anyhow!("Potentially dangerous test command detected"));
            }
        }
        
        Ok(())
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "Untitled Project".to_string(),
            project_type: ProjectType::Generic,
            root_path: PathBuf::new(),
            ignore_patterns: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                ".DS_Store".to_string(),
                "*.tmp".to_string(),
            ],
            include_patterns: vec!["*".to_string()],
            build_command: None,
            test_command: None,
            language_servers: Vec::new(),
        }
    }
}

/// Supported project types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Go,
    Java,
    CSharp,
    CPlusPlus,
    Web,
    Generic,
}

/// Language server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub file_extensions: Vec<String>,
}

/// File tree structure
#[derive(Debug, Clone)]
pub struct FileTree {
    pub root: PathBuf,
    pub files: Vec<FileEntry>,
    pub directories: Vec<DirectoryEntry>,
    pub total_files: usize,
    pub total_size: u64,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub size: u64,
    pub extension: Option<String>,
    pub is_text: bool,
}

#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub file_count: usize,
}

/// Symbol index for fast navigation
#[derive(Debug, Default)]
pub struct SymbolIndex {
    pub symbols: DashMap<String, Vec<Symbol>>,
    pub file_symbols: DashMap<PathBuf, Vec<Symbol>>,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub location: SymbolLocation,
    pub container: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SymbolKind {
    Function,
    Class,
    Interface,
    Variable,
    Constant,
    Module,
    Namespace,
    Property,
    Method,
    Struct,
    Enum,
    Trait,
}

#[derive(Debug, Clone)]
pub struct SymbolLocation {
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
    pub range: Option<(u32, u32)>, // (start_line, end_line)
}

/// Dependency graph analysis
#[derive(Debug, Default)]
pub struct DependencyGraph {
    dependencies: Vec<Dependency>,
    dev_dependencies: Vec<Dependency>,
    build_dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub source: DependencySource,
}

#[derive(Debug, Clone)]
pub enum DependencySource {
    Registry,
    Git { url: String, branch: Option<String> },
    Path { path: PathBuf },
}

/// File system events
#[derive(Debug, Clone)]
pub enum FileSystemEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed(PathBuf, PathBuf),
}

/// File watcher handle with proper lifecycle management
pub struct FileWatcherHandle {
    _watcher: RecommendedWatcher,
    shutdown_signal: Arc<AtomicBool>,
    task_handle: tokio::task::JoinHandle<()>,
}

impl FileWatcherHandle {
    pub async fn shutdown(self) -> Result<()> {
        self.shutdown_signal.store(true, Ordering::Relaxed);
        self.task_handle.await?;
        Ok(())
    }
}

/// Project structure
pub struct Project {
    pub id: ProjectId,
    pub root_path: PathBuf,
    pub config: ProjectConfig,
    pub file_tree: Arc<RwLock<FileTree>>,
    pub symbol_index: Arc<SymbolIndex>,
    pub dependencies: Arc<RwLock<DependencyGraph>>,
    pub last_indexed: Option<std::time::SystemTime>,
    pub file_watcher: Option<FileWatcherHandle>,
}

impl Project {
    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(watcher) = self.file_watcher.take() {
            watcher.shutdown().await?;
        }
        Ok(())
    }
}

/// Event processor with proper backpressure and error handling
pub struct EventProcessor {
    receiver: Arc<Mutex<Option<mpsc::Receiver<FileSystemEvent>>>>,
    shutdown_signal: Arc<AtomicBool>,
    max_events_per_second: usize,
    dropped_events_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl EventProcessor {
    pub fn new(capacity: usize, max_events_per_second: usize) -> (Self, mpsc::Sender<FileSystemEvent>) {
        let (sender, receiver) = mpsc::channel(capacity);
        
        (
            Self {
                receiver: Arc::new(Mutex::new(Some(receiver))),
                shutdown_signal: Arc::new(AtomicBool::new(false)),
                max_events_per_second,
                dropped_events_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            },
            sender,
        )
    }
    
    pub async fn start_processing<F>(&self, mut handler: F) -> Result<()>
    where
        F: FnMut(FileSystemEvent) -> Result<()> + Send + 'static,
    {
        let mut receiver = self.receiver.lock().await.take()
            .ok_or_else(|| anyhow::anyhow!("Event processor already started"))?;
        
        let shutdown_signal = self.shutdown_signal.clone();
        let max_events = self.max_events_per_second;
        let dropped_counter = self.dropped_events_counter.clone();
        
        tokio::spawn(async move {
            let mut last_second = std::time::Instant::now();
            let mut events_this_second = 0;
            
            while !shutdown_signal.load(Ordering::Relaxed) {
                let now = std::time::Instant::now();
                if now.duration_since(last_second) >= std::time::Duration::from_secs(1) {
                    if events_this_second >= max_events {
                        debug!("Rate limited {} events in the last second", events_this_second - max_events);
                    }
                    last_second = now;
                    events_this_second = 0;
                }
                
                match tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    receiver.recv()
                ).await {
                    Ok(Some(event)) => {
                        if events_this_second >= max_events {
                            dropped_counter.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        
                        events_this_second += 1;
                        
                        if let Err(e) = handler(event) {
                            error!("Error processing file system event: {}", e);
                        }
                    }
                    Ok(None) => {
                        debug!("Event channel closed");
                        break;
                    }
                    Err(_) => {
                        continue;
                    }
                }
            }
            
            info!("Event processor shutdown completed");
        });
        
        Ok(())
    }
    
    pub fn shutdown(&self) {
        self.shutdown_signal.store(true, Ordering::Relaxed);
    }
    
    pub fn get_dropped_events_count(&self) -> u64 {
        self.dropped_events_counter.load(Ordering::Relaxed)
    }
}

/// Configuration for project sandbox
#[derive(Debug, Clone)]
pub struct ProjectSandboxConfig {
    /// Allowed base directories for projects
    pub allowed_directories: Vec<PathBuf>,
    /// Maximum project size in bytes
    pub max_project_size: u64,
    /// Maximum file size in bytes
    pub max_file_size: u64,
    /// Maximum number of files in project
    pub max_file_count: usize,
}

impl Default for ProjectSandboxConfig {
    fn default() -> Self {
        let mut allowed_dirs = Vec::new();
        
        // Add user's home directory subdirectories with validation
        if let Some(home) = dirs::home_dir() {
            // List of subdirectories to allow under home
            let subdirs = [
                "Documents",
                "Projects", 
                "workspace",
                "dev",
                "src",
                "Desktop",
                "GitHub",
                "repos",
                "code",
            ];
            
            for subdir in &subdirs {
                let path = home.join(subdir);
                // Only add if directory exists and can be canonicalized
                if path.exists() && path.is_dir() {
                    if let Ok(canonical) = path.canonicalize() {
                        // Verify it's still under home after canonicalization (no symlink escape)
                        if let Ok(canonical_home) = home.canonicalize() {
                            if canonical.starts_with(&canonical_home) {
                                allowed_dirs.push(canonical);
                            }
                        }
                    }
                }
            }
        }
        
        // Add current working directory if safe
        if let Ok(cwd) = std::env::current_dir() {
            if let Ok(canonical_cwd) = cwd.canonicalize() {
                let cwd_str = canonical_cwd.to_string_lossy().to_lowercase();
                
                // Block system directories across all platforms
                let is_system = 
                    // Windows system directories
                    cwd_str.starts_with("c:\\windows") || 
                    cwd_str.starts_with("c:\\program files") ||
                    cwd_str.starts_with("c:\\programdata") ||
                    // Unix/Linux system directories
                    cwd_str.starts_with("/etc") || 
                    cwd_str.starts_with("/sys") ||
                    cwd_str.starts_with("/proc") ||
                    cwd_str.starts_with("/dev") ||
                    cwd_str.starts_with("/boot") ||
                    cwd_str.starts_with("/bin") ||
                    cwd_str.starts_with("/sbin") ||
                    cwd_str.starts_with("/lib") ||
                    cwd_str.starts_with("/usr/bin") ||
                    cwd_str.starts_with("/usr/sbin") ||
                    // macOS system directories
                    cwd_str.starts_with("/system") ||
                    cwd_str.starts_with("/library") ||
                    cwd_str.starts_with("/private");
                
                if !is_system {
                    allowed_dirs.push(canonical_cwd);
                }
            }
        }
        
        // Add temp directory for scratch projects (sandboxed)
        if let Some(temp_dir) = dirs::data_local_dir() {
            let atom_temp = temp_dir.join("atom-ide").join("projects");
            // Create if doesn't exist
            if !atom_temp.exists() {
                let _ = std::fs::create_dir_all(&atom_temp);
            }
            if let Ok(canonical) = atom_temp.canonicalize() {
                allowed_dirs.push(canonical);
            }
        }
        
        // Remove duplicates and sort
        allowed_dirs.sort();
        allowed_dirs.dedup();
        
        Self {
            allowed_directories: allowed_dirs,
            max_project_size: 10 * 1024 * 1024 * 1024, // 10GB
            max_file_size: 100 * 1024 * 1024, // 100MB
            max_file_count: 100_000,
        }
    }
}

/// Main project manager with proper resource management
pub struct ProjectManager {
    active_projects: Arc<DashMap<ProjectId, Project>>,
    event_processor: Arc<EventProcessor>,
    event_sender: mpsc::Sender<FileSystemEvent>,
    shutdown_signal: Arc<AtomicBool>,
    sandbox_config: ProjectSandboxConfig,
}

impl Default for ProjectManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectManager {
    pub fn new() -> Self {
        let (event_processor, event_sender) = EventProcessor::new(1000, 100);
        
        Self {
            active_projects: Arc::new(DashMap::new()),
            event_processor: Arc::new(event_processor),
            event_sender,
            shutdown_signal: Arc::new(AtomicBool::new(false)),
        }
    }
    
    pub async fn start(&self) -> Result<()> {
        let event_processor = self.event_processor.clone();
        let projects = Arc::clone(&self.active_projects);
        
        event_processor.start_processing(move |event| {
            debug!("Processing file system event: {:?}", event);
            
            match event {
                FileSystemEvent::Modified(path) => {
                    for project_entry in projects.iter() {
                        let project = project_entry.value();
                        if path.starts_with(&project.root_path) {
                            debug!("File modified in project {}: {:?}", project.config.name, path);
                        }
                    }
                }
                FileSystemEvent::Created(path) => {
                    debug!("File created: {:?}", path);
                }
                FileSystemEvent::Deleted(path) => {
                    debug!("File deleted: {:?}", path);
                }
                FileSystemEvent::Renamed(old_path, new_path) => {
                    debug!("File renamed: {:?} -> {:?}", old_path, new_path);
                }
            }
            
            Ok(())
        }).await?;
        
        Ok(())
    }
    
    /// Open a project from a given path with full validation
    pub async fn open_project(&self, path: PathBuf) -> Result<ProjectId> {
        info!("Opening project at: {:?}", path);
        
        // Validate and sanitize path
        let canonical_path = self.validate_and_canonicalize_path(&path)?;
        
        if !canonical_path.exists() {
            return Err(anyhow::anyhow!("Project path does not exist: {:?}", canonical_path));
        }
        
        if !canonical_path.is_dir() {
            return Err(anyhow::anyhow!("Project path is not a directory: {:?}", canonical_path));
        }
        
        let project_type = self.detect_project_type(&path).await?;
        let mut config = self.load_or_create_config(&path, project_type).await?;
        config.root_path = path.clone();
        
        config.validate()?;
        
        let project_id = ProjectId::new();
        
        let file_tree = self.scan_file_tree(&path, &config).await?;
        let dependencies = self.analyze_dependencies(&path, &config.project_type).await?;
        let file_watcher = self.setup_file_watching(&path).await?;
        
        let project = Project {
            id: project_id,
            root_path: path.clone(),
            config,
            file_tree: Arc::new(RwLock::new(file_tree)),
            symbol_index: Arc::new(SymbolIndex::default()),
            dependencies: Arc::new(RwLock::new(dependencies)),
            last_indexed: Some(std::time::SystemTime::now()),
            file_watcher: Some(file_watcher),
        };
        
        self.start_background_indexing(project_id, &project).await?;
        
        self.active_projects.insert(project_id, project);
        
        info!("Successfully opened project: {:?} with ID: {:?}", path, project_id);
        
        Ok(project_id)
    }
    
    /// Close a project with proper cleanup
    pub async fn close_project(&self, project_id: ProjectId) -> Result<bool> {
        if let Some((_, mut project)) = self.active_projects.remove(&project_id) {
            info!("Closing project: {:?}", project.root_path);
            project.shutdown().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Shutdown the project manager and all resources
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down ProjectManager");
        
        self.shutdown_signal.store(true, Ordering::Relaxed);
        self.event_processor.shutdown();
        
        let project_ids: Vec<ProjectId> = self.active_projects.iter()
            .map(|entry| *entry.key())
            .collect();
        
        for project_id in project_ids {
            self.close_project(project_id).await?;
        }
        
        info!("ProjectManager shutdown completed");
        Ok(())
    }
    
    pub fn get_project(&self, project_id: ProjectId) -> Option<dashmap::mapref::one::Ref<'_, ProjectId, Project>> {
        self.active_projects.get(&project_id)
    }
    
    pub fn list_projects(&self) -> Vec<ProjectId> {
        self.active_projects.iter().map(|entry| *entry.key()).collect()
    }
    
    async fn detect_project_type(&self, path: &PathBuf) -> Result<ProjectType> {
        debug!("Detecting project type for: {:?}", path);
        
        if path.join("Cargo.toml").exists() {
            return Ok(ProjectType::Rust);
        }
        
        if path.join("package.json").exists() {
            let package_json = path.join("package.json");
            if let Ok(content) = tokio::fs::read_to_string(&package_json).await {
                if content.contains("\"typescript\"") || path.join("tsconfig.json").exists() {
                    return Ok(ProjectType::TypeScript);
                } else {
                    return Ok(ProjectType::JavaScript);
                }
            }
        }
        
        if path.join("requirements.txt").exists() 
            || path.join("setup.py").exists() 
            || path.join("pyproject.toml").exists() {
            return Ok(ProjectType::Python);
        }
        
        if path.join("go.mod").exists() {
            return Ok(ProjectType::Go);
        }
        
        if path.join("pom.xml").exists() || path.join("build.gradle").exists() {
            return Ok(ProjectType::Java);
        }
        
        if path.join("CMakeLists.txt").exists() || path.join("Makefile").exists() {
            return Ok(ProjectType::CPlusPlus);
        }
        
        if path.join("index.html").exists() 
            || path.join("src").join("index.html").exists()
            || path.join("public").join("index.html").exists() {
            return Ok(ProjectType::Web);
        }
        
        Ok(ProjectType::Generic)
    }
    
    async fn load_or_create_config(&self, path: &PathBuf, project_type: ProjectType) -> Result<ProjectConfig> {
        let config_path = path.join(".atom-ide.toml");
        
        if config_path.exists() {
            match tokio::fs::read_to_string(&config_path).await {
                Ok(content) => {
                    if content.len() > 1024 * 1024 {
                        return Err(anyhow::anyhow!("Configuration file too large (max 1MB)"));
                    }
                    
                    match toml::from_str::<ProjectConfig>(&content) {
                        Ok(config) => {
                            config.validate()?;
                            debug!("Loaded existing project config");
                            return Ok(config);
                        }
                        Err(e) => {
                            warn!("Failed to parse project config, using defaults: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read project config file: {}", e);
                }
            }
        }
        
        let project_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled Project")
            .to_string();
        
        let mut config = ProjectConfig::default();
        config.name = project_name;
        config.project_type = project_type;
        config.root_path = path.clone();
        
        match config.project_type {
            ProjectType::Rust => {
                config.build_command = Some("cargo build".to_string());
                config.test_command = Some("cargo test".to_string());
                config.ignore_patterns.push("target/**".to_string());
            }
            ProjectType::JavaScript | ProjectType::TypeScript => {
                config.build_command = Some("npm run build".to_string());
                config.test_command = Some("npm test".to_string());
                config.ignore_patterns.push("node_modules/**".to_string());
                config.ignore_patterns.push("dist/**".to_string());
            }
            ProjectType::Python => {
                config.ignore_patterns.push("__pycache__/**".to_string());
                config.ignore_patterns.push("*.pyc".to_string());
                config.ignore_patterns.push(".venv/**".to_string());
                config.ignore_patterns.push("venv/**".to_string());
            }
            _ => {}
        }
        
        Ok(config)
    }
    
    async fn scan_file_tree(&self, path: &PathBuf, config: &ProjectConfig) -> Result<FileTree> {
        debug!("Scanning file tree for: {:?}", path);
        
        let path_clone = path.clone();
        let ignore_patterns = config.ignore_patterns.clone();
        
        let (files, directories, total_files, total_size) = tokio::task::spawn_blocking(move || {
            let mut files = Vec::new();
            let mut directories = Vec::new();
            let mut total_files = 0;
            let mut total_size = 0;
            
            for entry in WalkDir::new(&path_clone)
                .follow_links(false)
                .max_depth(10) // Prevent deep recursion
                .into_iter()
                .filter_entry(|e| {
                    let path_str = e.path().to_string_lossy();
                    !ignore_patterns.iter().any(|pattern| {
                        Self::pattern_matches(&path_str, pattern)
                    })
                }) {
                
                if let Ok(entry) = entry {
                    let entry_path = entry.path().to_path_buf();
                    
                    if let Ok(relative_path) = entry_path.strip_prefix(&path_clone) {
                        let relative_path = relative_path.to_path_buf();
                        
                        if entry_path.is_file() {
                            if let Ok(metadata) = entry.metadata() {
                                let size = metadata.len();
                                let extension = entry_path
                                    .extension()
                                    .and_then(|ext| ext.to_str())
                                    .map(|ext| ext.to_lowercase());
                                
                                let is_text = Self::is_text_file(&extension);
                                
                                files.push(FileEntry {
                                    path: entry_path,
                                    relative_path,
                                    size,
                                    extension,
                                    is_text,
                                });
                                
                                total_files += 1;
                                total_size += size;
                            }
                        } else if entry_path.is_dir() && entry_path != path_clone {
                            directories.push(DirectoryEntry {
                                path: entry_path,
                                relative_path,
                                file_count: 0,
                            });
                        }
                    }
                }
            }
            
            (files, directories, total_files, total_size)
        }).await?;
        
        info!("Scanned {} files and {} directories (total size: {} bytes)", 
              total_files, directories.len(), total_size);
        
        Ok(FileTree {
            root: path.clone(),
            files,
            directories,
            total_files,
            total_size,
        })
    }
    
    fn pattern_matches(path: &str, pattern: &str) -> bool {
        if pattern.ends_with("/**") {
            let prefix = &pattern[..pattern.len() - 3];
            path.contains(prefix)
        } else if pattern.starts_with("*.") {
            let suffix = &pattern[1..];
            path.ends_with(suffix)
        } else {
            path.contains(pattern)
        }
    }
    
    fn is_text_file(extension: &Option<String>) -> bool {
        match extension {
            Some(ext) => matches!(ext.as_str(),
                "rs" | "js" | "ts" | "py" | "go" | "java" | "cpp" | "c" | "h" | 
                "cs" | "php" | "rb" | "swift" | "kt" | "scala" | "clj" | "elm" |
                "html" | "css" | "scss" | "sass" | "less" | "xml" | "json" | 
                "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "md" | "txt" |
                "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd"
            ),
            None => false,
        }
    }
    
    async fn analyze_dependencies(&self, path: &PathBuf, project_type: &ProjectType) -> Result<DependencyGraph> {
        debug!("Analyzing dependencies for: {:?}", path);
        
        match project_type {
            ProjectType::Rust => self.analyze_rust_dependencies(path).await,
            ProjectType::JavaScript | ProjectType::TypeScript => self.analyze_npm_dependencies(path).await,
            ProjectType::Python => self.analyze_python_dependencies(path).await,
            _ => Ok(DependencyGraph::default()),
        }
    }
    
    async fn analyze_rust_dependencies(&self, path: &PathBuf) -> Result<DependencyGraph> {
        let cargo_toml = path.join("Cargo.toml");
        
        if !cargo_toml.exists() {
            return Ok(DependencyGraph::default());
        }
        
        let content = tokio::fs::read_to_string(&cargo_toml).await?;
        
        let parsed: toml::Value = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse Cargo.toml: {}", e))?;
        
        let mut dependencies = Vec::new();
        let mut dev_dependencies = Vec::new();
        let mut build_dependencies = Vec::new();
        
        if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_table()) {
            for (name, value) in deps {
                let dep = self.parse_cargo_dependency(name, value)?;
                dependencies.push(dep);
            }
        }
        
        if let Some(deps) = parsed.get("dev-dependencies").and_then(|d| d.as_table()) {
            for (name, value) in deps {
                let dep = self.parse_cargo_dependency(name, value)?;
                dev_dependencies.push(dep);
            }
        }
        
        if let Some(deps) = parsed.get("build-dependencies").and_then(|d| d.as_table()) {
            for (name, value) in deps {
                let dep = self.parse_cargo_dependency(name, value)?;
                build_dependencies.push(dep);
            }
        }
        
        Ok(DependencyGraph {
            dependencies,
            dev_dependencies,
            build_dependencies,
        })
    }
    
    fn parse_cargo_dependency(&self, name: &str, value: &toml::Value) -> Result<Dependency> {
        match value {
            toml::Value::String(version) => {
                Ok(Dependency {
                    name: name.to_string(),
                    version: version.clone(),
                    source: DependencySource::Registry,
                })
            }
            toml::Value::Table(table) => {
                let version = table.get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*")
                    .to_string();
                
                let source = if let Some(git) = table.get("git").and_then(|g| g.as_str()) {
                    let branch = table.get("branch").and_then(|b| b.as_str()).map(|s| s.to_string());
                    DependencySource::Git { 
                        url: git.to_string(), 
                        branch 
                    }
                } else if let Some(path_val) = table.get("path").and_then(|p| p.as_str()) {
                    DependencySource::Path { 
                        path: PathBuf::from(path_val) 
                    }
                } else {
                    DependencySource::Registry
                };
                
                Ok(Dependency {
                    name: name.to_string(),
                    version,
                    source,
                })
            }
            _ => Err(anyhow::anyhow!("Invalid dependency format for {}", name))
        }
    }
    
    async fn analyze_npm_dependencies(&self, path: &PathBuf) -> Result<DependencyGraph> {
        let package_json = path.join("package.json");
        
        if !package_json.exists() {
            return Ok(DependencyGraph::default());
        }
        
        let content = tokio::fs::read_to_string(&package_json).await?;
        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse package.json: {}", e))?;
        
        let mut dependencies = Vec::new();
        let mut dev_dependencies = Vec::new();
        
        if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_object()) {
            for (name, version) in deps {
                if let Some(version_str) = version.as_str() {
                    dependencies.push(Dependency {
                        name: name.clone(),
                        version: version_str.to_string(),
                        source: DependencySource::Registry,
                    });
                }
            }
        }
        
        if let Some(deps) = parsed.get("devDependencies").and_then(|d| d.as_object()) {
            for (name, version) in deps {
                if let Some(version_str) = version.as_str() {
                    dev_dependencies.push(Dependency {
                        name: name.clone(),
                        version: version_str.to_string(),
                        source: DependencySource::Registry,
                    });
                }
            }
        }
        
        Ok(DependencyGraph {
            dependencies,
            dev_dependencies,
            build_dependencies: Vec::new(),
        })
    }
    
    async fn analyze_python_dependencies(&self, path: &PathBuf) -> Result<DependencyGraph> {
        let mut dependencies = Vec::new();
        
        // Check requirements.txt
        let requirements_txt = path.join("requirements.txt");
        if requirements_txt.exists() {
            let content = tokio::fs::read_to_string(&requirements_txt).await?;
            
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') && !line.starts_with('-') {
                    let dep = self.parse_python_requirement(line)?;
                    dependencies.push(dep);
                }
            }
        }
        
        // Check pyproject.toml
        let pyproject_toml = path.join("pyproject.toml");
        if pyproject_toml.exists() {
            let content = tokio::fs::read_to_string(&pyproject_toml).await?;
            let parsed: toml::Value = toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse pyproject.toml: {}", e))?;
            
            if let Some(deps) = parsed
                .get("project")
                .and_then(|p| p.get("dependencies"))
                .and_then(|d| d.as_array()) {
                
                for dep_val in deps {
                    if let Some(dep_str) = dep_val.as_str() {
                        let dep = self.parse_python_requirement(dep_str)?;
                        dependencies.push(dep);
                    }
                }
            }
        }
        
        Ok(DependencyGraph {
            dependencies,
            dev_dependencies: Vec::new(),
            build_dependencies: Vec::new(),
        })
    }
    
    fn parse_python_requirement(&self, requirement: &str) -> Result<Dependency> {
        // Parse formats like "package==1.0.0", "package>=1.0.0", "package", etc.
        let parts: Vec<&str> = requirement.split(&['=', '>', '<', '!', '~'][..]).collect();
        let name = parts[0].trim().to_string();
        
        if name.is_empty() {
            return Err(anyhow::anyhow!("Invalid requirement format: {}", requirement));
        }
        
        let version = if parts.len() > 1 {
            parts[1..].join("").trim().to_string()
        } else {
            "*".to_string()
        };
        
        Ok(Dependency {
            name,
            version,
            source: DependencySource::Registry,
        })
    }
    
    async fn setup_file_watching(&self, path: &PathBuf) -> Result<FileWatcherHandle> {
        debug!("Setting up file watching for: {:?}", path);
        
        let sender = self.event_sender.clone();
        let path_clone = path.clone();
        let shutdown_signal = Arc::new(AtomicBool::new(false));
        let shutdown_signal_clone = shutdown_signal.clone();
        
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            match res {
                Ok(event) => {
                    let fs_event = match event.kind {
                        EventKind::Create(_) => {
                            if let Some(path) = event.paths.first() {
                                Some(FileSystemEvent::Created(path.clone()))
                            } else {
                                None
                            }
                        }
                        EventKind::Modify(_) => {
                            if let Some(path) = event.paths.first() {
                                Some(FileSystemEvent::Modified(path.clone()))
                            } else {
                                None
                            }
                        }
                        EventKind::Remove(_) => {
                            if let Some(path) = event.paths.first() {
                                Some(FileSystemEvent::Deleted(path.clone()))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    
                    if let Some(fs_event) = fs_event {
                        match sender.try_send(fs_event) {
                            Ok(()) => {},
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                // Channel is full - this is handled by rate limiting in EventProcessor
                            },
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                debug!("File watcher event channel closed");
                            }
                        }
                    }
                }
                Err(e) => error!("File watcher error: {}", e),
            }
        })?;
        
        watcher.watch(&path_clone, RecursiveMode::Recursive)?;
        
        let task_handle = tokio::spawn(async move {
            while !shutdown_signal_clone.load(Ordering::Relaxed) {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            debug!("File watcher task for {:?} shutdown", path_clone);
        });
        
        info!("Started file watching for: {:?}", path);
        
        Ok(FileWatcherHandle {
            _watcher: watcher,
            shutdown_signal,
            task_handle,
        })
    }
    
    async fn start_background_indexing(&self, project_id: ProjectId, _project: &Project) -> Result<()> {
        debug!("Starting background indexing for project: {:?}", project_id);
        
        let shutdown_signal = self.shutdown_signal.clone();
        
        tokio::spawn(async move {
            let mut indexing_interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            
            loop {
                tokio::select! {
                    _ = indexing_interval.tick() => {
                        if shutdown_signal.load(Ordering::Relaxed) {
                            break;
                        }
                        
                        debug!("Performing incremental indexing for project: {:?}", project_id);
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                        if shutdown_signal.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            }
            
            info!("Background indexing for project {:?} shutdown", project_id);
        });
        
        Ok(())
    }
    
    pub async fn search_files(&self, project_id: ProjectId, pattern: &str) -> Result<Vec<FileEntry>> {
        if let Some(project) = self.get_project(project_id) {
            let file_tree = project.file_tree.read().await;
            let pattern_lower = pattern.to_lowercase();
            
            let matching_files: Vec<FileEntry> = file_tree
                .files
                .iter()
                .filter(|file| {
                    file.relative_path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&pattern_lower)
                })
                .cloned()
                .collect();
            
            Ok(matching_files)
        } else {
            Err(anyhow::anyhow!("Project not found: {:?}", project_id))
        }
    }
    
    pub async fn get_project_stats(&self, project_id: ProjectId) -> Result<ProjectStats> {
        if let Some(project) = self.get_project(project_id) {
            let file_tree = project.file_tree.read().await;
            let dependencies = project.dependencies.read().await;
            
            let stats = ProjectStats {
                total_files: file_tree.total_files,
                total_size: file_tree.total_size,
                total_dependencies: dependencies.dependencies.len() + 
                                   dependencies.dev_dependencies.len() + 
                                   dependencies.build_dependencies.len(),
                last_indexed: project.last_indexed,
            };
            
            Ok(stats)
        } else {
            Err(anyhow::anyhow!("Project not found: {:?}", project_id))
        }
    }
    
    pub fn get_dropped_events_count(&self) -> u64 {
        self.event_processor.get_dropped_events_count()
    }
}

impl Drop for ProjectManager {
    fn drop(&mut self) {
        self.shutdown_signal.store(true, Ordering::Relaxed);
        self.event_processor.shutdown();
    }
}

/// Project statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStats {
    pub total_files: usize,
    pub total_size: u64,
    pub total_dependencies: usize,
    pub last_indexed: Option<std::time::SystemTime>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;
    
    #[tokio::test]
    async fn test_project_manager_creation() {
        let manager = ProjectManager::new();
        assert_eq!(manager.list_projects().len(), 0);
    }
    
    #[tokio::test]
    async fn test_detect_rust_project() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_path = temp_dir.path().to_path_buf();
        
        fs::write(project_path.join("Cargo.toml"), "[package]\nname = \"test\"\nversion = \"0.1.0\"").await?;
        
        let manager = ProjectManager::new();
        let project_type = manager.detect_project_type(&project_path).await?;
        
        assert_eq!(project_type, ProjectType::Rust);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_project_lifecycle() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_path = temp_dir.path().canonicalize()?;
        
        fs::write(project_path.join("Cargo.toml"), "[package]\nname = \"test\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1.0\"").await?;
        fs::create_dir(project_path.join("src")).await?;
        fs::write(project_path.join("src").join("main.rs"), "fn main() { println!(\"Hello, world!\"); }").await?;
        
        let manager = ProjectManager::new();
        manager.start().await?;
        
        let project_id = manager.open_project(project_path).await?;
        assert_eq!(manager.list_projects().len(), 1);
        
        let stats = manager.get_project_stats(project_id).await?;
        assert!(stats.total_files >= 2);
        assert!(stats.total_dependencies >= 1);
        
        let closed = manager.close_project(project_id).await?;
        assert!(closed);
        assert_eq!(manager.list_projects().len(), 0);
        
        manager.shutdown().await?;
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_search_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_path = temp_dir.path().canonicalize()?;
        
        fs::write(project_path.join("main.rs"), "fn main() {}").await?;
        fs::write(project_path.join("lib.rs"), "pub fn lib() {}").await?;
        fs::write(project_path.join("config.toml"), "[config]").await?;
        
        let manager = ProjectManager::new();
        manager.start().await?;
        
        let project_id = manager.open_project(project_path).await?;
        
        let rust_files = manager.search_files(project_id, "rs").await?;
        assert_eq!(rust_files.len(), 2);
        
        let main_files = manager.search_files(project_id, "main").await?;
        assert_eq!(main_files.len(), 1);
        
        manager.shutdown().await?;
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_dependency_parsing() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let project_path = temp_dir.path().canonicalize()?;
        
        let cargo_toml = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }
local-dep = { path = "../local" }

[dev-dependencies]
criterion = "0.5"
"#;
        
        fs::write(project_path.join("Cargo.toml"), cargo_toml).await?;
        
        let manager = ProjectManager::new();
        let deps = manager.analyze_rust_dependencies(&project_path).await?;
        
        assert_eq!(deps.dependencies.len(), 3);
        assert_eq!(deps.dev_dependencies.len(), 1);
        
        let serde_dep = deps.dependencies.iter().find(|d| d.name == "serde").unwrap();
        assert_eq!(serde_dep.version, "1.0");
        assert!(matches!(serde_dep.source, DependencySource::Registry));
        
        let local_dep = deps.dependencies.iter().find(|d| d.name == "local-dep").unwrap();
        assert!(matches!(local_dep.source, DependencySource::Path { .. }));
        
        Ok(())
    }
}