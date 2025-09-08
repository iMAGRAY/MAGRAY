use anyhow::Result;
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, Tree};
use tracing::{debug, error, info, warn};
use crate::project_manager::{ProjectId, Symbol, SymbolKind, SymbolLocation, SymbolIndex};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{Duration, Instant};
use regex::Regex;
use glob::Pattern;

/// Language-specific parsers and queries
pub struct LanguageSupport {
    pub language: Language,
    pub parser: Parser,
    pub symbol_query_str: String,
    pub file_extensions: Vec<String>,
}

impl LanguageSupport {
    pub fn new(language: Language, symbol_query_text: &str, extensions: Vec<String>) -> Result<Self> {
        let mut parser = Parser::new();
        parser.set_language(language)
            .map_err(|e| anyhow::anyhow!("Failed to set language: {}", e))?;
        
        Ok(Self {
            language,
            parser,
            symbol_query_str: symbol_query_text.to_string(),
            file_extensions: extensions,
        })
    }
}

/// High-performance indexing engine with symbol extraction
pub struct IndexingEngine {
    language_supports: DashMap<String, LanguageSupport>,
    index_cache: Arc<DashMap<PathBuf, IndexedFile>>,
    shutdown_signal: Arc<AtomicBool>,
    performance_metrics: Arc<RwLock<IndexingMetrics>>,
}

#[derive(Debug, Clone)]
pub struct IndexedFile {
    pub path: PathBuf,
    pub last_modified: std::time::SystemTime,
    pub symbols: Vec<Symbol>,
    pub size: u64,
    pub checksum: u64,
    pub parse_duration: Duration,
}

#[derive(Debug, Default, Clone)]
pub struct IndexingMetrics {
    pub files_indexed: u64,
    pub total_symbols: u64,
    pub total_parse_time: Duration,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub errors_encountered: u64,
}

impl Default for IndexingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexingEngine {
    pub fn new() -> Self {
        let engine = Self {
            language_supports: DashMap::new(),
            index_cache: Arc::new(DashMap::new()),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            performance_metrics: Arc::new(RwLock::new(IndexingMetrics::default())),
        };
        
        // Initialize language supports
        engine.initialize_language_supports();
        
        engine
    }
    
    fn initialize_language_supports(&self) {
        // Rust language support
        if let Ok(rust_support) = Self::create_rust_support() {
            self.language_supports.insert("rust".to_string(), rust_support);
        } else {
            warn!("Failed to initialize Rust language support");
        }
        
        // JavaScript language support
        if let Ok(js_support) = Self::create_javascript_support() {
            self.language_supports.insert("javascript".to_string(), js_support);
        } else {
            warn!("Failed to initialize JavaScript language support");
        }
        
        // TypeScript language support
        if let Ok(ts_support) = Self::create_typescript_support() {
            self.language_supports.insert("typescript".to_string(), ts_support);
        } else {
            warn!("Failed to initialize TypeScript language support");
        }
        
        // Python language support
        if let Ok(py_support) = Self::create_python_support() {
            self.language_supports.insert("python".to_string(), py_support);
        } else {
            warn!("Failed to initialize Python language support");
        }
        
        info!("Initialized {} language supports", self.language_supports.len());
    }
    
    fn create_rust_support() -> Result<LanguageSupport> {
        let language = tree_sitter_rust::language();
        let query_text = r#"
            (function_item name: (identifier) @function.name) @function.definition
            (impl_item type: (type_identifier) @impl.type) @impl.definition
            (struct_item name: (type_identifier) @struct.name) @struct.definition
            (enum_item name: (type_identifier) @enum.name) @enum.definition
            (trait_item name: (type_identifier) @trait.name) @trait.definition
            (mod_item name: (identifier) @module.name) @module.definition
            (const_item name: (identifier) @constant.name) @constant.definition
            (static_item name: (identifier) @static.name) @static.definition
            (type_item name: (type_identifier) @type.name) @type.definition
            (macro_definition name: (identifier) @macro.name) @macro.definition
        "#;
        
        LanguageSupport::new(
            language,
            query_text,
            vec!["rs".to_string()],
        )
    }
    
    fn create_javascript_support() -> Result<LanguageSupport> {
        let language = tree_sitter_javascript::language();
        let query_text = r#"
            (function_declaration name: (identifier) @function.name) @function.definition
            (method_definition name: (property_identifier) @method.name) @method.definition
            (class_declaration name: (identifier) @class.name) @class.definition
            (variable_declarator name: (identifier) @variable.name) @variable.definition
            (export_statement (function_declaration name: (identifier) @export.function.name)) @export.function.definition
            (export_statement (class_declaration name: (identifier) @export.class.name)) @export.class.definition
            (arrow_function) @arrow_function.definition
        "#;
        
        LanguageSupport::new(
            language,
            query_text,
            vec!["js".to_string(), "jsx".to_string(), "mjs".to_string()],
        )
    }
    
    fn create_typescript_support() -> Result<LanguageSupport> {
        let language = tree_sitter_typescript::language_typescript();
        let query_text = r#"
            (function_declaration name: (identifier) @function.name) @function.definition
            (method_definition name: (property_identifier) @method.name) @method.definition
            (class_declaration name: (type_identifier) @class.name) @class.definition
            (interface_declaration name: (type_identifier) @interface.name) @interface.definition
            (type_alias_declaration name: (type_identifier) @type.name) @type.definition
            (enum_declaration name: (identifier) @enum.name) @enum.definition
            (variable_declarator name: (identifier) @variable.name) @variable.definition
            (export_statement (function_declaration name: (identifier) @export.function.name)) @export.function.definition
            (export_statement (class_declaration name: (type_identifier) @export.class.name)) @export.class.definition
            (export_statement (interface_declaration name: (type_identifier) @export.interface.name)) @export.interface.definition
        "#;
        
        LanguageSupport::new(
            language,
            query_text,
            vec!["ts".to_string(), "tsx".to_string()],
        )
    }
    
    fn create_python_support() -> Result<LanguageSupport> {
        let language = tree_sitter_python::language();
        let query_text = r#"
            (function_definition name: (identifier) @function.name) @function.definition
            (class_definition name: (identifier) @class.name) @class.definition
            (assignment left: (identifier) @variable.name) @variable.definition
            (import_statement name: (dotted_name (identifier) @import.name)) @import.definition
            (import_from_statement name: (dotted_name (identifier) @import.name)) @import.definition
            (decorated_definition (function_definition name: (identifier) @decorated_function.name)) @decorated_function.definition
            (decorated_definition (class_definition name: (identifier) @decorated_class.name)) @decorated_class.definition
        "#;
        
        LanguageSupport::new(
            language,
            query_text,
            vec!["py".to_string(), "pyw".to_string()],
        )
    }
    
    /// Index a single file and extract symbols
    pub async fn index_file(&self, file_path: &Path) -> Result<Vec<Symbol>> {
        let start_time = Instant::now();
        
        // Check if file is cached and up to date
        if let Some(cached) = self.get_cached_file(file_path).await? {
            let mut metrics = self.performance_metrics.write().await;
            metrics.cache_hits += 1;
            debug!("Cache hit for file: {:?}", file_path);
            return Ok(cached.symbols);
        }
        
        let language_id = self.detect_language(file_path)?;
        let content = tokio::fs::read_to_string(file_path).await
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;
        
        let symbols = self.parse_and_extract_symbols(&language_id, &content, file_path).await?;
        
        // Cache the result
        let metadata = tokio::fs::metadata(file_path).await?;
        let parse_duration = start_time.elapsed();
        
        let indexed_file = IndexedFile {
            path: file_path.to_path_buf(),
            last_modified: metadata.modified()?,
            symbols: symbols.clone(),
            size: metadata.len(),
            checksum: self.calculate_checksum(&content),
            parse_duration,
        };
        
        self.index_cache.insert(file_path.to_path_buf(), indexed_file);
        
        // Update metrics
        let mut metrics = self.performance_metrics.write().await;
        metrics.files_indexed += 1;
        metrics.total_symbols += symbols.len() as u64;
        metrics.total_parse_time += parse_duration;
        metrics.cache_misses += 1;
        
        debug!(
            "Indexed file {:?}: {} symbols in {:?}",
            file_path,
            symbols.len(),
            parse_duration
        );
        
        Ok(symbols)
    }
    
    async fn get_cached_file(&self, file_path: &Path) -> Result<Option<IndexedFile>> {
        if let Some(cached_entry) = self.index_cache.get(file_path) {
            let cached = cached_entry.value().clone();
            
            // Check if file has been modified
            let metadata = tokio::fs::metadata(file_path).await?;
            let current_modified = metadata.modified()?;
            
            if cached.last_modified >= current_modified && cached.size == metadata.len() {
                return Ok(Some(cached));
            } else {
                // Remove outdated cache entry
                self.index_cache.remove(file_path);
            }
        }
        
        Ok(None)
    }
    
    fn detect_language(&self, file_path: &Path) -> Result<String> {
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();
        
        for lang_entry in self.language_supports.iter() {
            let (lang_name, support) = lang_entry.pair();
            if support.file_extensions.contains(&extension) {
                return Ok(lang_name.clone());
            }
        }
        
        Err(anyhow::anyhow!("Unsupported file extension: {}", extension))
    }
    
    async fn parse_and_extract_symbols(&self, language_id: &str, content: &str, file_path: &Path) -> Result<Vec<Symbol>> {
        let language_support = self.language_supports
            .get(language_id)
            .ok_or_else(|| anyhow::anyhow!("Language support not found: {}", language_id))?;
        
        // Parse the source code - use spawn_blocking for CPU-intensive parsing
        let content_owned = content.to_string();
        let file_path_owned = file_path.to_path_buf();
        let language = language_support.language;
        let query_str = language_support.symbol_query_str.clone();
        
        tokio::task::spawn_blocking(move || {
            let mut parser = Parser::new();
            parser.set_language(language)
                .map_err(|e| anyhow::anyhow!("Failed to set language: {}", e))?;
            
            // Create Query from the string in blocking context
            let query = Query::new(language, &query_str)
                .map_err(|e| anyhow::anyhow!("Failed to create query: {}", e))?;
            
            let tree = parser.parse(&content_owned, None)
                .ok_or_else(|| anyhow::anyhow!("Failed to parse file"))?;
            
            Self::extract_symbols_from_tree(&tree, &query, &content_owned, &file_path_owned)
        }).await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }
    
    fn extract_symbols_from_tree(tree: &Tree, query: &Query, content: &str, file_path: &Path) -> Result<Vec<Symbol>> {
        let mut symbols = Vec::new();
        let mut cursor = QueryCursor::new();
        let captures = cursor.matches(query, tree.root_node(), content.as_bytes());
        
        for capture_match in captures {
            for capture in capture_match.captures {
                let node = capture.node;
                let capture_name = &query.capture_names()[capture.index as usize];
                
                if let Some(symbol) = Self::create_symbol_from_capture(node, &capture_name, content, file_path)? {
                    symbols.push(symbol);
                }
            }
        }
        
        // Sort symbols by location for better performance
        symbols.sort_by_key(|s| (s.location.line, s.location.column));
        
        Ok(symbols)
    }
    
    fn create_symbol_from_capture(node: Node, capture_name: &str, content: &str, file_path: &Path) -> Result<Option<Symbol>> {
        let symbol_text = node.utf8_text(content.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to extract symbol text: {}", e))?;
        
        let start_position = node.start_position();
        let end_position = node.end_position();
        
        let (symbol_kind, should_include) = match capture_name {
            name if name.contains("function") => (SymbolKind::Function, true),
            name if name.contains("class") => (SymbolKind::Class, true),
            name if name.contains("interface") => (SymbolKind::Interface, true),
            name if name.contains("struct") => (SymbolKind::Struct, true),
            name if name.contains("enum") => (SymbolKind::Enum, true),
            name if name.contains("trait") => (SymbolKind::Trait, true),
            name if name.contains("method") => (SymbolKind::Method, true),
            name if name.contains("variable") => (SymbolKind::Variable, true),
            name if name.contains("constant") => (SymbolKind::Constant, true),
            name if name.contains("module") => (SymbolKind::Module, true),
            name if name.contains("namespace") => (SymbolKind::Namespace, true),
            name if name.contains("property") => (SymbolKind::Property, true),
            _ => (SymbolKind::Variable, false), // Skip unknown symbol types
        };
        
        if !should_include {
            return Ok(None);
        }
        
        // Extract container information (parent function, class, etc.)
        let container = Self::find_container(node, content);
        
        let symbol = Symbol {
            name: symbol_text.to_string(),
            kind: symbol_kind,
            location: SymbolLocation {
                file: file_path.to_path_buf(),
                line: start_position.row as u32,
                column: start_position.column as u32,
                range: Some((start_position.row as u32, end_position.row as u32)),
            },
            container,
        };
        
        Ok(Some(symbol))
    }
    
    fn find_container(node: Node, content: &str) -> Option<String> {
        let mut current = node.parent();
        
        while let Some(parent) = current {
            match parent.kind() {
                "function_item" | "function_declaration" | "method_definition" => {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        if let Ok(name) = name_node.utf8_text(content.as_bytes()) {
                            return Some(name.to_string());
                        }
                    }
                }
                "impl_item" | "class_declaration" | "struct_item" => {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        if let Ok(name) = name_node.utf8_text(content.as_bytes()) {
                            return Some(name.to_string());
                        }
                    }
                }
                "mod_item" | "module" => {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        if let Ok(name) = name_node.utf8_text(content.as_bytes()) {
                            return Some(format!("mod::{}", name));
                        }
                    }
                }
                _ => {}
            }
            current = parent.parent();
        }
        
        None
    }
    
    /// Index multiple files concurrently with controlled parallelism
    pub async fn index_files_parallel(self: &Arc<Self>, file_paths: &[PathBuf], max_concurrent: usize) -> Result<Vec<Symbol>> {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
        let mut tasks = Vec::new();
        
        for file_path in file_paths {
            let semaphore = semaphore.clone();
            let file_path = file_path.clone();
            let engine = Arc::clone(self);
            
            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.map_err(|e| anyhow::anyhow!("Semaphore error: {}", e))?;
                engine.index_file(&file_path).await
            });
            
            tasks.push(task);
        }
        
        // Collect results
        let mut all_symbols = Vec::new();
        for task in tasks {
            match task.await {
                Ok(Ok(symbols)) => all_symbols.extend(symbols),
                Ok(Err(e)) => {
                    error!("Failed to index file: {}", e);
                    let mut metrics = self.performance_metrics.write().await;
                    metrics.errors_encountered += 1;
                }
                Err(e) => {
                    error!("Task join error: {}", e);
                    let mut metrics = self.performance_metrics.write().await;
                    metrics.errors_encountered += 1;
                }
            }
            
            // Check for shutdown signal
            if self.shutdown_signal.load(Ordering::Relaxed) {
                break;
            }
        }
        
        Ok(all_symbols)
    }
    
    /// Update symbol index with new symbols
    pub async fn update_symbol_index(&self, project_id: ProjectId, symbols: Vec<Symbol>, symbol_index: &Arc<SymbolIndex>) {
        for symbol in symbols {
            // Index by name for fast lookup
            symbol_index.symbols
                .entry(symbol.name.clone())
                .or_insert_with(Vec::new)
                .push(symbol.clone());
            
            // Index by file for file-specific queries
            symbol_index.file_symbols
                .entry(symbol.location.file.clone())
                .or_insert_with(Vec::new)
                .push(symbol);
        }
        
        debug!("Updated symbol index for project: {:?}", project_id);
    }
    
    /// Search symbols by name with fuzzy matching
    pub fn search_symbols(&self, symbol_index: &Arc<SymbolIndex>, query: &str, max_results: usize) -> Vec<Symbol> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        let mut scores = Vec::new();
        
        for entry in symbol_index.symbols.iter() {
            let (name, symbols) = entry.pair();
            let name_lower = name.to_lowercase();
            
            // Calculate relevance score
            let score = if name_lower == query_lower {
                100 // Exact match
            } else if name_lower.starts_with(&query_lower) {
                80  // Prefix match
            } else if name_lower.contains(&query_lower) {
                60  // Contains match
            } else {
                // Fuzzy match using simple string distance
                Self::fuzzy_match_score(&name_lower, &query_lower)
            };
            
            if score > 30 { // Threshold for relevance
                for symbol in symbols.iter() {
                    results.push(symbol.clone());
                    scores.push(score);
                }
            }
        }
        
        // Sort by score and take top results
        let mut indexed_results: Vec<(usize, Symbol)> = results.into_iter().enumerate().collect();
        indexed_results.sort_by(|a, b| scores[b.0].cmp(&scores[a.0]));
        
        indexed_results
            .into_iter()
            .take(max_results)
            .map(|(_, symbol)| symbol)
            .collect()
    }
    
    fn fuzzy_match_score(text: &str, pattern: &str) -> u32 {
        // Simple fuzzy matching - can be improved with more sophisticated algorithms
        let mut score = 0u32;
        let mut pattern_index = 0;
        
        for ch in text.chars() {
            if pattern_index < pattern.len() {
                let pattern_chars: Vec<char> = pattern.chars().collect();
                if ch == pattern_chars[pattern_index] {
                    score += 10;
                    pattern_index += 1;
                }
            }
        }
        
        // Bonus for consecutive matches
        if pattern_index == pattern.len() {
            score += 20;
        }
        
        score
    }
    
    /// Filter files using improved pattern matching
    pub fn filter_files_with_patterns(&self, files: &[PathBuf], ignore_patterns: &[String]) -> Vec<PathBuf> {
        let compiled_patterns: Vec<_> = ignore_patterns
            .iter()
            .filter_map(|pattern| {
                match self.compile_pattern(pattern) {
                    Ok(compiled) => Some(compiled),
                    Err(e) => {
                        warn!("Failed to compile pattern '{}': {}", pattern, e);
                        None
                    }
                }
            })
            .collect();
        
        files
            .iter()
            .filter(|file_path| {
                let path_str = file_path.to_string_lossy();
                
                !compiled_patterns.iter().any(|pattern| {
                    match pattern {
                        CompiledPattern::Glob(glob) => glob.matches(&path_str),
                        CompiledPattern::Regex(regex) => regex.is_match(&path_str),
                        CompiledPattern::Simple(simple) => path_str.contains(simple),
                    }
                })
            })
            .cloned()
            .collect()
    }
    
    fn compile_pattern(&self, pattern: &str) -> Result<CompiledPattern> {
        // Handle different pattern types
        if pattern.contains('*') || pattern.contains('?') {
            // Glob pattern
            let glob = Pattern::new(pattern)
                .map_err(|e| anyhow::anyhow!("Invalid glob pattern '{}': {}", pattern, e))?;
            Ok(CompiledPattern::Glob(glob))
        } else if pattern.starts_with('^') || pattern.contains("\\d") || pattern.contains("\\w") {
            // Regex pattern
            let regex = Regex::new(pattern)
                .map_err(|e| anyhow::anyhow!("Invalid regex pattern '{}': {}", pattern, e))?;
            Ok(CompiledPattern::Regex(regex))
        } else {
            // Simple string matching
            Ok(CompiledPattern::Simple(pattern.to_string()))
        }
    }
    
    fn calculate_checksum(&self, content: &str) -> u64 {
        // Simple checksum using FNV-1a hash algorithm
        let mut hash = 0xcbf29ce484222325u64;
        for byte in content.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
    
    /// Get performance metrics
    pub async fn get_metrics(&self) -> IndexingMetrics {
        self.performance_metrics.read().await.clone()
    }
    
    /// Clear cache entries for files
    pub fn clear_cache(&self, files: Option<&[PathBuf]>) {
        if let Some(files) = files {
            for file in files {
                self.index_cache.remove(file);
            }
        } else {
            self.index_cache.clear();
        }
    }
    
    pub fn shutdown(&self) {
        self.shutdown_signal.store(true, Ordering::Relaxed);
        info!("IndexingEngine shutdown initiated");
    }
}

#[derive(Debug)]
enum CompiledPattern {
    Glob(Pattern),
    Regex(Regex),
    Simple(String),
}

impl Drop for IndexingEngine {
    fn drop(&mut self) {
        self.shutdown_signal.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;
    
    #[tokio::test]
    async fn test_indexing_engine_creation() {
        let engine = IndexingEngine::new();
        assert!(engine.language_supports.len() > 0);
    }
    
    #[tokio::test]
    async fn test_rust_file_indexing() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let rust_file = temp_dir.path().join("test.rs");
        
        let rust_code = r#"
fn main() {
    println!("Hello, world!");
}

struct TestStruct {
    field: i32,
}

impl TestStruct {
    fn new(field: i32) -> Self {
        Self { field }
    }
    
    fn get_field(&self) -> i32 {
        self.field
    }
}

enum Color {
    Red,
    Green,
    Blue,
}

trait Display {
    fn display(&self) -> String;
}
        "#;
        
        fs::write(&rust_file, rust_code).await?;
        
        let engine = IndexingEngine::new();
        let symbols = engine.index_file(&rust_file).await?;
        
        assert!(symbols.len() > 0);
        
        // Check for expected symbols
        let symbol_names: Vec<_> = symbols.iter().map(|s| &s.name).collect();
        assert!(symbol_names.contains(&&"main".to_string()));
        assert!(symbol_names.contains(&&"TestStruct".to_string()));
        assert!(symbol_names.contains(&&"Color".to_string()));
        
        // Check symbol types
        let function_symbols: Vec<_> = symbols.iter()
            .filter(|s| matches!(s.kind, SymbolKind::Function))
            .collect();
        assert!(function_symbols.len() > 0);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_caching_functionality() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("cache_test.rs");
        
        fs::write(&test_file, "fn test() {}").await?;
        
        let engine = IndexingEngine::new();
        
        // First indexing - should be cache miss
        let start = Instant::now();
        let symbols1 = engine.index_file(&test_file).await?;
        let first_duration = start.elapsed();
        
        // Second indexing - should be cache hit
        let start = Instant::now();
        let symbols2 = engine.index_file(&test_file).await?;
        let second_duration = start.elapsed();
        
        assert_eq!(symbols1.len(), symbols2.len());
        assert!(second_duration < first_duration); // Cache hit should be faster
        
        let metrics = engine.get_metrics().await;
        assert!(metrics.cache_hits > 0);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_parallel_indexing() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut files = Vec::new();
        
        // Create multiple test files
        for i in 0..5 {
            let file_path = temp_dir.path().join(format!("test_{}.rs", i));
            fs::write(&file_path, format!("fn test_{}() {{}}", i)).await?;
            files.push(file_path);
        }
        
        let engine = Arc::new(IndexingEngine::new());
        let symbols = engine.index_files_parallel(&files, 3).await?;
        
        // Tree-sitter may find multiple symbols per file (functions, modules, etc.)
        // Just verify we found at least the expected functions
        assert!(symbols.len() >= 5, "Should find at least 5 symbols, found {}", symbols.len());
        
        // Verify we found all our test functions
        for i in 0..5 {
            let function_name = format!("test_{}", i);
            assert!(
                symbols.iter().any(|s| s.name == function_name),
                "Should find function {}", function_name
            );
        }
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_symbol_search() -> Result<()> {
        let symbol_index = Arc::new(SymbolIndex::default());
        
        // Add test symbols
        let test_symbols = vec![
            Symbol {
                name: "test_function".to_string(),
                kind: SymbolKind::Function,
                location: SymbolLocation {
                    file: PathBuf::from("test.rs"),
                    line: 1,
                    column: 0,
                    range: None,
                },
                container: None,
            },
            Symbol {
                name: "TestStruct".to_string(),
                kind: SymbolKind::Struct,
                location: SymbolLocation {
                    file: PathBuf::from("test.rs"),
                    line: 5,
                    column: 0,
                    range: None,
                },
                container: None,
            },
        ];
        
        for symbol in test_symbols {
            symbol_index.symbols
                .entry(symbol.name.clone())
                .or_insert_with(Vec::new)
                .push(symbol);
        }
        
        let engine = IndexingEngine::new();
        
        // Test exact match - fuzzy search may find multiple matches
        let results = engine.search_symbols(&symbol_index, "test_function", 10);
        assert!(results.len() >= 1, "Should find at least one match");
        assert!(results.iter().any(|s| s.name == "test_function"), "Should find test_function");
        
        // Test partial match
        let results = engine.search_symbols(&symbol_index, "test", 10);
        assert!(results.len() >= 1);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_pattern_filtering() -> Result<()> {
        let engine = IndexingEngine::new();
        
        let files = vec![
            PathBuf::from("src/main.rs"),
            PathBuf::from("target/debug/main"),
            PathBuf::from("node_modules/package/index.js"),
            PathBuf::from("tests/test.rs"),
            PathBuf::from("README.md"),
        ];
        
        let ignore_patterns = vec![
            "target/**".to_string(),
            "node_modules/**".to_string(),
            "*.md".to_string(),
        ];
        
        let filtered = engine.filter_files_with_patterns(&files, &ignore_patterns);
        
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains(&PathBuf::from("src/main.rs")));
        assert!(filtered.contains(&PathBuf::from("tests/test.rs")));
        
        Ok(())
    }
}