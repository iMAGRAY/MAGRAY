//! Atom IDE Indexing Engine
//!
//! This crate provides search and indexing functionality using Tantivy
//! for persistent indexing and ripgrep for ad-hoc searches.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tantivy::{
    collector::TopDocs,
    directory::MmapDirectory,
    query::QueryParser,
    schema::{Field, Schema, STORED, TEXT},
    Index, IndexWriter, ReloadPolicy,
};
use tokio::process::Command;
use tracing::{error, info, warn};

/// Index-related errors
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("Tantivy error: {0}")]
    TantivyError(#[from] tantivy::TantivyError),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Index not found: {0}")]
    IndexNotFound(String),
    #[error("Search error: {0}")]
    SearchError(String),
    #[error("Settings error: {0}")]
    SettingsError(#[from] atom_settings::SettingsError),
    #[error("Directory error: {0}")]
    DirectoryError(#[from] tantivy::directory::error::OpenDirectoryError),
}

/// Search result from index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// File path
    pub path: String,
    /// Line number (1-based)
    pub line: usize,
    /// Column number (0-based)
    pub column: usize,
    /// Line content
    pub content: String,
    /// Matched text
    pub matched_text: String,
    /// Relevance score
    pub score: f32,
}

/// Search options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    /// Case sensitive search
    pub case_sensitive: bool,
    /// Whole word matching
    pub whole_word: bool,
    /// Use regex patterns
    pub use_regex: bool,
    /// File include patterns
    pub include_patterns: Vec<String>,
    /// File exclude patterns
    pub exclude_patterns: Vec<String>,
    /// Maximum number of results
    pub max_results: usize,
    /// Search context lines
    pub context_lines: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
            include_patterns: vec!["*".to_string()],
            exclude_patterns: vec![
                "*.git/*".to_string(),
                "*node_modules/*".to_string(),
                "*target/*".to_string(),
                "*.log".to_string(),
            ],
            max_results: 1000,
            context_lines: 0,
        }
    }
}

/// Main indexing engine
pub struct IndexEngine {
    /// Tantivy index
    index: Index,
    /// Index writer
    writer: Option<IndexWriter>,
    /// Schema fields
    fields: IndexFields,
    /// Query parser
    query_parser: QueryParser,
    /// Settings
    settings: atom_settings::Settings,
    /// Index directory
    index_dir: PathBuf,
}

/// Tantivy schema fields
#[derive(Debug, Clone)]
struct IndexFields {
    path: Field,
    content: Field,
    line_number: Field,
    file_type: Field,
}

impl IndexEngine {
    /// Create new index engine
    pub async fn new(
        index_dir: PathBuf,
        settings: atom_settings::Settings,
    ) -> Result<Self, IndexError> {
        // Create schema
        let mut schema_builder = Schema::builder();
        let fields = IndexFields {
            path: schema_builder.add_text_field("path", TEXT | STORED),
            content: schema_builder.add_text_field("content", TEXT),
            line_number: schema_builder.add_u64_field("line_number", STORED),
            file_type: schema_builder.add_text_field("file_type", TEXT | STORED),
        };
        let schema = schema_builder.build();

        // Create or open index
        let index = if index_dir.exists() {
            Index::open_in_dir(&index_dir)?
        } else {
            std::fs::create_dir_all(&index_dir)?;
            let directory = MmapDirectory::open(&index_dir)?;
            let settings = tantivy::IndexSettings::default();
            Index::create(directory, schema.clone(), settings)?
        };

        // Create query parser
        let query_parser = QueryParser::for_index(&index, vec![fields.content]);

        info!("Index engine initialized at: {:?}", index_dir);

        Ok(Self {
            index,
            writer: None,
            fields,
            query_parser,
            settings,
            index_dir,
        })
    }

    /// Start indexing session (get writer)
    pub async fn start_indexing(&mut self) -> Result<(), IndexError> {
        if self.writer.is_some() {
            warn!("Indexing session already active");
            return Ok(());
        }

        let writer = self.index.writer(50_000_000)?; // 50MB heap
        self.writer = Some(writer);

        info!("Started indexing session");
        Ok(())
    }

    /// Finish indexing session (commit and close writer)
    pub async fn finish_indexing(&mut self) -> Result<(), IndexError> {
        if let Some(mut writer) = self.writer.take() {
            writer.commit()?;
            info!("Committed index changes");
        } else {
            warn!("No active indexing session to finish");
        }

        Ok(())
    }

    /// Index a single file
    pub async fn index_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), IndexError> {
        let path = path.as_ref();

        if let Some(ref mut writer) = self.writer {
            // Read file content
            let content = match tokio::fs::read_to_string(path).await {
                Ok(content) => content,
                Err(e) => {
                    warn!("Failed to read file {:?}: {}", path, e);
                    return Ok(()); // Skip unreadable files
                }
            };

            // Detect file type
            let file_type = path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("unknown")
                .to_string();

            // Index line by line for better search granularity
            for (line_num, line_content) in content.lines().enumerate() {
                if !line_content.trim().is_empty() {
                    // Create a new document using the Document type from tantivy
                    let doc = tantivy::doc!(
                        self.fields.path => path.to_string_lossy().to_string(),
                        self.fields.content => line_content.to_string(),
                        self.fields.line_number => (line_num + 1) as u64,
                        self.fields.file_type => file_type.clone()
                    );

                    writer.add_document(doc)?;
                }
            }

            info!(
                "Indexed file: {:?} ({} lines)",
                path,
                content.lines().count()
            );
        } else {
            return Err(IndexError::SearchError(
                "No active indexing session".to_string(),
            ));
        }

        Ok(())
    }

    /// Search using Tantivy index
    pub async fn search_index(
        &self,
        query_str: &str,
        options: &SearchOptions,
    ) -> Result<Vec<SearchResult>, IndexError> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        let searcher = reader.searcher();

        // Parse query
        let query = self
            .query_parser
            .parse_query(query_str)
            .map_err(|e| IndexError::SearchError(format!("Failed to parse query: {}", e)))?;

        // Search
        let top_docs = searcher.search(&query, &TopDocs::with_limit(options.max_results))?;

        let mut results = Vec::new();

        for (score, doc_address) in top_docs {
            let retrieved_doc = searcher.doc(doc_address)?;

            let path = retrieved_doc
                .get_first(self.fields.path)
                .and_then(|v| v.as_text())
                .unwrap_or("unknown")
                .to_string();

            let content = retrieved_doc
                .get_first(self.fields.content)
                .and_then(|v| v.as_text())
                .unwrap_or("")
                .to_string();

            let line = retrieved_doc
                .get_first(self.fields.line_number)
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize;

            // Find matched text (simplified)
            let matched_text = if content.to_lowercase().contains(&query_str.to_lowercase()) {
                query_str.to_string()
            } else {
                content.chars().take(50).collect()
            };

            results.push(SearchResult {
                path,
                line,
                column: 0, // TODO: Calculate actual column
                content,
                matched_text,
                score,
            });
        }

        info!(
            "Index search found {} results for '{}'",
            results.len(),
            query_str
        );
        Ok(results)
    }

    /// Search using ripgrep (ad-hoc search)
    pub async fn search_ripgrep(
        &self,
        query: &str,
        root_path: &Path,
        options: &SearchOptions,
    ) -> Result<Vec<SearchResult>, IndexError> {
        let mut cmd = Command::new("rg");

        // Basic ripgrep options
        cmd.arg("--line-number")
            .arg("--column")
            .arg("--no-heading")
            .arg("--with-filename")
            .arg("--color=never")
            .arg("--max-count")
            .arg(options.max_results.to_string());

        // Case sensitivity
        if !options.case_sensitive {
            cmd.arg("--ignore-case");
        }

        // Whole word matching
        if options.whole_word {
            cmd.arg("--word-regexp");
        }

        // Regex or literal
        if !options.use_regex {
            cmd.arg("--fixed-strings");
        }

        // Include/exclude patterns
        for pattern in &options.exclude_patterns {
            cmd.arg("--glob").arg(format!("!{}", pattern));
        }

        for pattern in &options.include_patterns {
            if pattern != "*" {
                cmd.arg("--glob").arg(pattern);
            }
        }

        // Context lines
        if options.context_lines > 0 {
            cmd.arg("-C").arg(options.context_lines.to_string());
        }

        cmd.arg(query).arg(root_path);

        // Execute command
        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(IndexError::SearchError(format!(
                "Ripgrep failed: {}",
                stderr
            )));
        }

        // Parse ripgrep output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();

        for line in stdout.lines() {
            if let Some(result) = self.parse_ripgrep_line(line, query) {
                results.push(result);
            }
        }

        info!(
            "Ripgrep search found {} results for '{}'",
            results.len(),
            query
        );
        Ok(results)
    }

    /// Parse a single line from ripgrep output
    fn parse_ripgrep_line(&self, line: &str, query: &str) -> Option<SearchResult> {
        // Format: path:line:column:content
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() < 4 {
            return None;
        }

        let path = parts[0].to_string();
        let line_num = parts[1].parse::<usize>().ok()?;
        let column = parts[2].parse::<usize>().ok()?;
        let content = parts[3].to_string();

        Some(SearchResult {
            path,
            line: line_num,
            column,
            content: content.clone(),
            matched_text: query.to_string(),
            score: 1.0, // Default score for ripgrep results
        })
    }

    /// Get index statistics
    pub async fn get_stats(&self) -> Result<IndexStats, IndexError> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();

        // Get index modification time with proper error handling
        let last_updated = match std::fs::metadata(&self.index_dir) {
            Ok(metadata) => match metadata.modified() {
                Ok(modified_time) => Some(modified_time),
                Err(e) => {
                    // On some filesystems, modified time may not be available
                    warn!("Index modification time not available: {}", e);
                    None
                }
            },
            Err(e) => {
                // This indicates a more serious problem with the index directory
                error!(
                    "Failed to access index directory {:?}: {}",
                    self.index_dir, e
                );
                return Err(IndexError::IoError(e));
            }
        };

        let stats = IndexStats {
            num_documents: searcher.num_docs(),
            index_size_bytes: self.calculate_index_size()?,
            last_updated,
        };

        Ok(stats)
    }

    /// Calculate index size on disk
    fn calculate_index_size(&self) -> Result<u64, IndexError> {
        let mut total_size = 0u64;

        fn visit_dir(dir: &Path, total: &mut u64) -> Result<(), std::io::Error> {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    visit_dir(&path, total)?;
                } else {
                    *total += entry.metadata()?.len();
                }
            }
            Ok(())
        }

        visit_dir(&self.index_dir, &mut total_size)?;
        Ok(total_size)
    }
}

/// Index statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexStats {
    pub num_documents: u64,
    pub index_size_bytes: u64,
    pub last_updated: Option<std::time::SystemTime>,
}
