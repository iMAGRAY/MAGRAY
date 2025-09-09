//! Atom IDE Core Functionality
//!
//! This crate provides core functionality for Atom IDE including
//! text buffer management, syntax parsing with tree-sitter, and configuration.

use ropey::Rope;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tree_sitter::{Language, Parser, Tree};

/// Core errors
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("IO error: {0}")]
    IoErrorString(String),
    #[error("Tree-sitter parsing error: {0}")]
    ParseError(String),
    #[error("Buffer not found: {0}")]
    BufferNotFound(String),
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),
    #[error("Settings error: {0}")]
    SettingsError(#[from] atom_settings::SettingsError),
}

/// Text buffer with rope data structure
#[derive(Debug, Clone)]
pub struct TextBuffer {
    /// Unique buffer identifier
    pub id: String,
    /// File path (if any)
    pub path: Option<PathBuf>,
    /// Rope-based text content
    pub content: Rope,
    /// Language for syntax highlighting
    pub language: Option<String>,
    /// Whether buffer has unsaved changes
    pub is_dirty: bool,
    /// Syntax tree (if parsed)
    pub syntax_tree: Option<Tree>,
    /// Buffer encoding
    pub encoding: String,
    /// Line ending style
    pub line_ending: LineEnding,
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_tokio_runtime() {
        // Basic test to verify Tokio runtime is working
        let result =
            tokio::time::timeout(tokio::time::Duration::from_millis(100), async { "success" })
                .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }
}

/// Line ending styles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LineEnding {
    /// Unix-style (LF)
    Unix,
    /// Windows-style (CRLF)
    Windows,
    /// Classic Mac-style (CR)
    Mac,
}

/// Text position in buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// Text range in buffer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Text edit operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    /// Range to replace
    pub range: Range,
    /// New text content
    pub new_text: String,
}

/// Buffer manager for handling multiple text buffers
pub struct BufferManager {
    buffers: HashMap<String, TextBuffer>,
    parsers: HashMap<String, Parser>,
    #[allow(dead_code)]
    languages: HashMap<String, Language>,
    #[allow(dead_code)]
    settings: atom_settings::Settings,
    next_buffer_id: usize,
}

impl BufferManager {
    /// Create new buffer manager
    pub fn new(settings: atom_settings::Settings) -> Self {
        Self {
            buffers: HashMap::new(),
            parsers: HashMap::new(),
            languages: HashMap::new(),
            settings,
            next_buffer_id: 1,
        }
    }

    /// Open file and create buffer
    pub async fn open_file<P: AsRef<Path>>(&mut self, path: P) -> Result<String, CoreError> {
        let path = path.as_ref();
        let path_buf = path.to_path_buf();

        // Check if buffer already exists for this file
        for (id, buffer) in &self.buffers {
            if let Some(ref buffer_path) = buffer.path {
                if buffer_path == &path_buf {
                    return Ok(id.clone());
                }
            }
        }

        // Read file content
        let content = fs::read_to_string(&path).await?;

        // Detect line endings
        let line_ending = Self::detect_line_ending(&content);

        // Detect language from file extension
        let language = Self::detect_language(&path_buf);

        // Create buffer
        let buffer_id = self.generate_buffer_id();
        let mut buffer = TextBuffer {
            id: buffer_id.clone(),
            path: Some(path_buf),
            content: Rope::from_str(&content),
            language: language.clone(),
            is_dirty: false,
            syntax_tree: None,
            encoding: "UTF-8".to_string(),
            line_ending,
        };

        // Parse syntax if language is supported
        if let Some(ref lang) = language {
            match self.parse_buffer(&mut buffer, lang).await {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("Failed to parse syntax for {}: {}", buffer_id, e);
                }
            }
        }

        self.buffers.insert(buffer_id.clone(), buffer);

        tracing::info!("Opened file: {} (buffer_id: {})", path.display(), buffer_id);
        Ok(buffer_id)
    }

    /// Create new empty buffer
    pub fn new_buffer(&mut self) -> String {
        let buffer_id = self.generate_buffer_id();
        let buffer = TextBuffer {
            id: buffer_id.clone(),
            path: None,
            content: Rope::new(),
            language: None,
            is_dirty: false,
            syntax_tree: None,
            encoding: "UTF-8".to_string(),
            line_ending: LineEnding::Unix,
        };

        self.buffers.insert(buffer_id.clone(), buffer);
        tracing::info!("Created new buffer: {}", buffer_id);
        buffer_id
    }

    /// Save buffer to file
    pub async fn save_buffer(
        &mut self,
        buffer_id: &str,
        path: Option<&Path>,
    ) -> Result<(), CoreError> {
        let (save_path, content, line_ending) = {
            let buffer = self
                .buffers
                .get(buffer_id)
                .ok_or_else(|| CoreError::BufferNotFound(buffer_id.to_string()))?;

            let requested_path = match path {
                Some(p) => p.to_path_buf(),
                None => buffer
                    .path
                    .as_ref()
                    .ok_or_else(|| CoreError::BufferNotFound("No path specified".to_string()))?
                    .clone(),
            };

            // Security: Validate and canonicalize path to prevent path traversal
            let save_path = self.validate_save_path(&requested_path)?;

            // Extract needed data to avoid borrow conflicts
            let content = buffer.content.clone();
            let line_ending = buffer.line_ending.clone();

            (save_path, content, line_ending)
        };

        // Convert rope to string with appropriate line endings
        let content_str = self.rope_to_string_with_endings(&content, &line_ending);

        // Create parent directory if needed (within workspace bounds)
        if let Some(parent) = save_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    CoreError::IoErrorString(format!(
                        "Cannot create directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
        }

        // Write file with proper error handling
        fs::write(&save_path, content_str).await.map_err(|e| {
            CoreError::IoErrorString(format!("Cannot write to {}: {}", save_path.display(), e))
        })?;

        // Update buffer state after successful write
        let buffer = self
            .buffers
            .get_mut(buffer_id)
            .ok_or_else(|| CoreError::BufferNotFound(buffer_id.to_string()))?;
        buffer.path = Some(save_path.clone());
        buffer.is_dirty = false;

        tracing::info!("Saved buffer {} to {}", buffer_id, save_path.display());
        Ok(())
    }

    /// Get buffer by ID
    pub fn get_buffer(&self, buffer_id: &str) -> Option<&TextBuffer> {
        self.buffers.get(buffer_id)
    }

    /// Get mutable buffer by ID
    pub fn get_buffer_mut(&mut self, buffer_id: &str) -> Option<&mut TextBuffer> {
        self.buffers.get_mut(buffer_id)
    }

    /// Apply text edit to buffer
    pub async fn apply_edit(&mut self, buffer_id: &str, edit: TextEdit) -> Result<(), CoreError> {
        // Get language first to avoid borrow conflicts
        let language = {
            let buffer = self
                .buffers
                .get(buffer_id)
                .ok_or_else(|| CoreError::BufferNotFound(buffer_id.to_string()))?;
            buffer.language.clone()
        };

        // Apply edit to buffer
        {
            let buffer = self
                .buffers
                .get_mut(buffer_id)
                .ok_or_else(|| CoreError::BufferNotFound(buffer_id.to_string()))?;

            // Convert positions to byte indices using static helper
            let content_clone = buffer.content.clone();
            let start_idx = Self::position_to_byte_idx_static(&content_clone, edit.range.start);
            let end_idx = Self::position_to_byte_idx_static(&content_clone, edit.range.end);

            // Apply edit to rope
            buffer.content.remove(start_idx..end_idx);
            buffer.content.insert(start_idx, &edit.new_text);
            buffer.is_dirty = true;
        }

        // Re-parse syntax if needed
        if let Some(language) = language {
            // Extract content and old tree before parsing to avoid borrow conflicts
            let (content_str, old_tree) = {
                let buffer = self
                    .buffers
                    .get(buffer_id)
                    .ok_or_else(|| CoreError::BufferNotFound(buffer_id.to_string()))?;
                (buffer.content.to_string(), buffer.syntax_tree.clone())
            };

            // Create or get parser for language
            let mut parser = Parser::new();
            match language.to_lowercase().as_str() {
                "rust" => {
                    let rust_language = tree_sitter_rust::LANGUAGE.into();
                    parser.set_language(&rust_language).map_err(|e| {
                        CoreError::ParseError(format!("Failed to set Rust language: {}", e))
                    })?;
                }
                "javascript" => {
                    let js_language = tree_sitter_javascript::LANGUAGE.into();
                    parser.set_language(&js_language).map_err(|e| {
                        CoreError::ParseError(format!("Failed to set JavaScript language: {}", e))
                    })?;
                }
                "typescript" => {
                    let ts_language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
                    parser.set_language(&ts_language).map_err(|e| {
                        CoreError::ParseError(format!("Failed to set TypeScript language: {}", e))
                    })?;
                }
                "python" => {
                    let py_language = tree_sitter_python::LANGUAGE.into();
                    parser.set_language(&py_language).map_err(|e| {
                        CoreError::ParseError(format!("Failed to set Python language: {}", e))
                    })?;
                }
                "json" => {
                    let json_language = tree_sitter_json::LANGUAGE.into();
                    parser.set_language(&json_language).map_err(|e| {
                        CoreError::ParseError(format!("Failed to set JSON language: {}", e))
                    })?;
                }
                "markdown" => {
                    // Markdown support temporarily disabled due to tree-sitter ABI incompatibility
                    // Skip parsing but don't fail
                    return Ok(());
                }
                _ => {
                    // Language not supported, skip parsing
                    return Ok(());
                }
            }

            // Parse with the configured parser
            match parser.parse(&content_str, old_tree.as_ref()) {
                Some(tree) => {
                    if let Some(buffer) = self.buffers.get_mut(buffer_id) {
                        buffer.syntax_tree = Some(tree);
                    }
                }
                None => {
                    tracing::warn!(
                        "Failed to parse buffer {} for language {}",
                        buffer_id,
                        language
                    );
                }
            }
        }

        Ok(())
    }

    /// Close buffer
    pub fn close_buffer(&mut self, buffer_id: &str) -> Result<(), CoreError> {
        self.buffers
            .remove(buffer_id)
            .ok_or_else(|| CoreError::BufferNotFound(buffer_id.to_string()))?;

        tracing::info!("Closed buffer: {}", buffer_id);
        Ok(())
    }

    /// Get all buffer IDs
    pub fn buffer_ids(&self) -> Vec<String> {
        self.buffers.keys().cloned().collect()
    }

    /// Generate unique buffer ID
    fn generate_buffer_id(&mut self) -> String {
        let id = format!("buffer_{}", self.next_buffer_id);
        self.next_buffer_id += 1;
        id
    }

    /// Detect line ending style from content
    fn detect_line_ending(content: &str) -> LineEnding {
        if content.contains("\r\n") {
            LineEnding::Windows
        } else if content.contains('\r') {
            LineEnding::Mac
        } else {
            LineEnding::Unix
        }
    }

    /// Detect language from file path
    fn detect_language(path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext.to_lowercase().as_str() {
                "rs" => Some("rust"),
                "js" | "jsx" => Some("javascript"),
                "ts" | "tsx" => Some("typescript"),
                "py" => Some("python"),
                "go" => Some("go"),
                "c" => Some("c"),
                "cpp" | "cxx" | "cc" => Some("cpp"),
                "h" | "hpp" => Some("c"),
                "java" => Some("java"),
                "json" => Some("json"),
                "toml" => Some("toml"),
                "yaml" | "yml" => Some("yaml"),
                "html" => Some("html"),
                "css" => Some("css"),
                "md" => Some("markdown"),
                _ => None,
            })
            .map(|s| s.to_string())
    }

    /// Parse buffer syntax with tree-sitter
    async fn parse_buffer(
        &mut self,
        buffer: &mut TextBuffer,
        language: &str,
    ) -> Result<(), CoreError> {
        // Get or create parser for this language
        let parser = self.get_or_create_parser(language)?;

        // Parse the buffer content
        let content_str = buffer.content.to_string();
        let tree = parser
            .parse(&content_str, buffer.syntax_tree.as_ref())
            .ok_or_else(|| CoreError::ParseError("Failed to parse buffer".to_string()))?;

        buffer.syntax_tree = Some(tree);
        Ok(())
    }

    /// Get or create parser for language
    fn get_or_create_parser(&mut self, language: &str) -> Result<&mut Parser, CoreError> {
        if !self.parsers.contains_key(language) {
            let mut parser = Parser::new();

            // Set language-specific tree-sitter parser
            match language.to_lowercase().as_str() {
                "rust" => {
                    let rust_language = tree_sitter_rust::LANGUAGE.into();
                    parser.set_language(&rust_language).map_err(|e| {
                        CoreError::ParseError(format!("Failed to set Rust language: {}", e))
                    })?;
                    tracing::info!("Initialized Rust parser");
                }
                "javascript" => {
                    let js_language = tree_sitter_javascript::LANGUAGE.into();
                    parser.set_language(&js_language).map_err(|e| {
                        CoreError::ParseError(format!("Failed to set JavaScript language: {}", e))
                    })?;
                    tracing::info!("Initialized JavaScript parser");
                }
                "typescript" => {
                    let ts_language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
                    parser.set_language(&ts_language).map_err(|e| {
                        CoreError::ParseError(format!("Failed to set TypeScript language: {}", e))
                    })?;
                    tracing::info!("Initialized TypeScript parser");
                }
                "python" => {
                    let py_language = tree_sitter_python::LANGUAGE.into();
                    parser.set_language(&py_language).map_err(|e| {
                        CoreError::ParseError(format!("Failed to set Python language: {}", e))
                    })?;
                    tracing::info!("Initialized Python parser");
                }
                "json" => {
                    let json_language = tree_sitter_json::LANGUAGE.into();
                    parser.set_language(&json_language).map_err(|e| {
                        CoreError::ParseError(format!("Failed to set JSON language: {}", e))
                    })?;
                    tracing::info!("Initialized JSON parser");
                }
                "markdown" => {
                    // Markdown support temporarily disabled due to tree-sitter ABI incompatibility
                    tracing::warn!("Markdown syntax highlighting temporarily disabled");
                    // Don't fail, just use parser without language set
                }
                _ => {
                    tracing::warn!("Language '{}' not supported, using generic parser without syntax highlighting", language);
                    // Don't set a language for unsupported types - parser will work as plain text
                }
            }

            self.parsers.insert(language.to_string(), parser);
        }

        Ok(self
            .parsers
            .get_mut(language)
            .expect("Parser must exist after successful insertion"))
    }

    /// Convert position to byte index in rope
    #[allow(dead_code)]
    fn position_to_byte_idx(&self, rope: &Rope, position: Position) -> usize {
        Self::position_to_byte_idx_static(rope, position)
    }

    /// Convert position to byte index in rope (static version)
    fn position_to_byte_idx_static(rope: &Rope, position: Position) -> usize {
        let line_start = rope.line_to_byte(position.line.min(rope.len_lines().saturating_sub(1)));
        let line = rope.line(position.line);
        let column_bytes = line
            .byte_slice(0..position.column.min(line.len_chars()))
            .len_bytes();
        line_start + column_bytes
    }

    /// Validate and canonicalize save path to prevent path traversal attacks
    fn validate_save_path(&self, requested_path: &Path) -> Result<PathBuf, CoreError> {
        // Get current working directory as workspace root
        let workspace_root = std::env::current_dir().map_err(|e| {
            CoreError::IoErrorString(format!("Cannot get current directory: {}", e))
        })?;

        // Resolve path relative to workspace root if it's relative
        let resolved_path = if requested_path.is_absolute() {
            // For absolute paths, ensure they're within workspace bounds
            requested_path.to_path_buf()
        } else {
            workspace_root.join(requested_path)
        };

        // Canonicalize to resolve .. and symlinks
        let canonical_path = resolved_path.canonicalize().or_else(|_| {
            // If file doesn't exist yet, canonicalize parent and append filename
            if let Some(parent) = resolved_path.parent() {
                if let Some(filename) = resolved_path.file_name() {
                    match parent.canonicalize() {
                        Ok(canonical_parent) => Ok(canonical_parent.join(filename)),
                        Err(e) => Err(CoreError::IoErrorString(format!(
                            "Cannot validate path: {}",
                            e
                        ))),
                    }
                } else {
                    Err(CoreError::IoErrorString("Invalid file path".to_string()))
                }
            } else {
                Err(CoreError::IoErrorString("Cannot resolve path".to_string()))
            }
        })?;

        // Security check: ensure canonical path is within workspace
        let canonical_workspace = workspace_root.canonicalize().map_err(|e| {
            CoreError::IoErrorString(format!("Cannot canonicalize workspace: {}", e))
        })?;

        if !canonical_path.starts_with(&canonical_workspace) {
            return Err(CoreError::IoErrorString(format!(
                "Path traversal detected: {} is outside workspace {}",
                canonical_path.display(),
                canonical_workspace.display()
            )));
        }

        Ok(canonical_path)
    }

    /// Convert rope to string with specific line endings
    fn rope_to_string_with_endings(&self, rope: &Rope, line_ending: &LineEnding) -> String {
        let content = rope.to_string();
        match line_ending {
            LineEnding::Unix => content.replace("\r\n", "\n").replace('\r', "\n"),
            LineEnding::Windows => content
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .replace('\n', "\r\n"),
            LineEnding::Mac => content.replace("\r\n", "\n").replace('\n', "\r"),
        }
    }
}

impl Default for LineEnding {
    fn default() -> Self {
        #[cfg(windows)]
        return LineEnding::Windows;
        #[cfg(not(windows))]
        return LineEnding::Unix;
    }
}
