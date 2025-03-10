//! Search engine implementation

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use arrayvec::ArrayVec;

use crate::error::{Error, Result};
use crate::types::SearchResult;

/// Maximum number of search results to return
pub const MAX_RESULTS: usize = 100;

/// Maximum number of documents to store
pub const MAX_DOCUMENTS: usize = 100;

/// Maximum length of stored content in bytes
pub const MAX_CONTENT_LENGTH: usize = 1_000;

/// Maximum number of terms in a query
pub const MAX_QUERY_TERMS: usize = 10;

/// Maximum term length in bytes
pub const MAX_TERM_LENGTH: usize = 50;

/// Maximum path length in bytes
pub const MAX_PATH_BYTES: usize = 1024;

/// Index file magic number
pub const MAGIC: [u8; 4] = *b"SNAP";

/// Index file version
pub const VERSION: u8 = 1;

/// Document in the search index with fixed-size buffers
#[derive(Debug)]
struct Document {
    /// Path to the document
    path:    PathBuf,
    /// Content of the document as raw bytes
    content: ArrayVec<u8, MAX_CONTENT_LENGTH>,
}

/// Simple search engine with fixed-size indices and zero post-init allocation
#[derive(Debug)]
pub struct SearchEngine {
    /// Documents in the index
    documents: Box<ArrayVec<Document, MAX_DOCUMENTS>>,
}

impl SearchEngine {
    /// Creates a new search engine
    #[must_use = "SearchEngine must be used to store and search documents"]
    pub fn new() -> Self {
        Self { documents: Box::new(ArrayVec::new()) }
    }

    /// Load an existing search engine from an index file
    ///
    /// # Errors
    /// Returns error if:
    /// - File cannot be read
    /// - File format is invalid
    /// - Document limit would be exceeded
    pub fn load(path: &Path) -> Result<Self> {
        let mut file =
            File::open(path).map_err(|e| Error::Search(format!("Failed to open index: {e}")))?;

        // Read and verify header
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)
            .map_err(|e| Error::Search(format!("Failed to read magic: {e}")))?;
        if magic != MAGIC {
            return Err(Error::Search("Invalid index file format".into()));
        }

        let mut version = [0u8; 1];
        file.read_exact(&mut version)
            .map_err(|e| Error::Search(format!("Failed to read version: {e}")))?;
        if version[0] != VERSION {
            return Err(Error::Search(format!("Unsupported index version: {}", version[0])));
        }

        let mut ndocs = [0u8; 4];
        file.read_exact(&mut ndocs)
            .map_err(|e| Error::Search(format!("Failed to read document count: {e}")))?;
        let ndocs = u32::from_le_bytes(ndocs) as usize;
        if ndocs > MAX_DOCUMENTS {
            return Err(Error::Search("Too many documents in index".into()));
        }

        let mut engine = Self::new();

        // Read documents
        for _ in 0..ndocs {
            // Read path
            let mut path_len = [0u8; 2];
            file.read_exact(&mut path_len)
                .map_err(|e| Error::Search(format!("Failed to read path length: {e}")))?;
            let path_len = u16::from_le_bytes(path_len) as usize;
            if path_len > MAX_PATH_BYTES {
                return Err(Error::Search("Path too long".into()));
            }

            let mut path_buf = ArrayVec::<u8, MAX_PATH_BYTES>::new();
            for _ in 0..path_len {
                let mut byte = [0u8; 1];
                file.read_exact(&mut byte)
                    .map_err(|e| Error::Search(format!("Failed to read path: {e}")))?;
                path_buf.try_push(byte[0]).map_err(|_| Error::Search("Path too long".into()))?;
            }

            // Read content
            let mut content_len = [0u8; 2];
            file.read_exact(&mut content_len)
                .map_err(|e| Error::Search(format!("Failed to read content length: {e}")))?;
            let content_len = u16::from_le_bytes(content_len) as usize;
            if content_len > MAX_CONTENT_LENGTH {
                return Err(Error::Search("Content too large".into()));
            }

            let mut content = ArrayVec::new();
            for _ in 0..content_len {
                let mut byte = [0u8; 1];
                file.read_exact(&mut byte)
                    .map_err(|e| Error::Search(format!("Failed to read content: {e}")))?;
                content.try_push(byte[0]).map_err(|_| Error::Search("Content too large".into()))?;
            }

            // Create document
            let path_str = String::from_utf8_lossy(&path_buf).into_owned();
            let path = PathBuf::from(path_str);
            engine
                .documents
                .try_push(Document { path, content })
                .map_err(|_| Error::Search("Too many documents".into()))?;
        }

        Ok(engine)
    }

    /// Save the search engine to an index file
    ///
    /// # Errors
    /// Returns error if:
    /// - File cannot be written
    /// - Path lengths exceed limits
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut file = File::create(path)
            .map_err(|e| Error::Search(format!("Failed to create index: {e}")))?;

        // Write header
        file.write_all(&MAGIC).map_err(|e| Error::Search(format!("Failed to write magic: {e}")))?;
        file.write_all(&[VERSION])
            .map_err(|e| Error::Search(format!("Failed to write version: {e}")))?;

        let ndocs = u32::try_from(self.documents.len())
            .map_err(|_| Error::Search("Too many documents for index format".into()))?;
        file.write_all(&ndocs.to_le_bytes())
            .map_err(|e| Error::Search(format!("Failed to write document count: {e}")))?;

        // Write documents
        for doc in self.documents.iter() {
            let path_str = doc.path.to_string_lossy();
            let path_bytes = path_str.as_bytes();
            if path_bytes.len() > MAX_PATH_BYTES {
                return Err(Error::Search("Path too long".into()));
            }

            // Write path
            let path_len = u16::try_from(path_bytes.len())
                .map_err(|_| Error::Search("Path too long for index format".into()))?;
            file.write_all(&path_len.to_le_bytes())
                .map_err(|e| Error::Search(format!("Failed to write path length: {e}")))?;
            file.write_all(path_bytes)
                .map_err(|e| Error::Search(format!("Failed to write path: {e}")))?;

            // Write content
            let content_len = u16::try_from(doc.content.len())
                .map_err(|_| Error::Search("Content too large for index format".into()))?;
            file.write_all(&content_len.to_le_bytes())
                .map_err(|e| Error::Search(format!("Failed to write content length: {e}")))?;
            file.write_all(&doc.content)
                .map_err(|e| Error::Search(format!("Failed to write content: {e}")))?;
        }

        Ok(())
    }

    /// Add a document to the index
    ///
    /// # Errors
    /// Returns error if:
    /// - Document limit exceeded
    /// - Content too large
    pub fn add_document(&mut self, path: &Path, content: &str) -> Result<()> {
        let mut doc_content = ArrayVec::new();
        for &b in content.as_bytes() {
            doc_content.try_push(b).map_err(|_| Error::Search("Content too large".into()))?;
        }

        self.documents
            .try_push(Document { path: path.to_path_buf(), content: doc_content })
            .map_err(|_| Error::Search("Too many documents".into()))
    }

    /// Check if a term matches content at word boundaries
    fn term_matches(term: &[u8], content: &[u8]) -> bool {
        if term.is_empty() || content.is_empty() || term.len() > content.len() {
            return false;
        }

        // Convert term to lowercase for case-insensitive comparison
        let mut term_lower = ArrayVec::<u8, MAX_TERM_LENGTH>::new();
        for &b in term {
            if term_lower.try_push(b.to_ascii_lowercase()).is_err() {
                return false;
            }
        }

        // Convert content to lowercase and look for word-boundary matches
        let mut content_lower = ArrayVec::<u8, MAX_CONTENT_LENGTH>::new();
        for &b in content {
            if content_lower.try_push(b.to_ascii_lowercase()).is_err() {
                return false;
            }
        }

        let content_lower = &content_lower;
        let term_lower = &term_lower;

        // Check each possible position for a word-boundary match
        for i in 0..=content_lower.len().saturating_sub(term_lower.len()) {
            // Check if we're at a word boundary
            let is_start = i == 0 || !content_lower[i - 1].is_ascii_alphanumeric();
            let is_end = i + term_lower.len() == content_lower.len()
                || !content_lower[i + term_lower.len()].is_ascii_alphanumeric();

            if is_start && is_end {
                // Check for match at this position
                let mut matches = true;
                for (a, b) in term_lower.iter().zip(&content_lower[i..]) {
                    if a != b {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    return true;
                }
            }
        }
        false
    }

    /// Calculate normalized relevance score for a document
    fn calculate_score(query: &str, doc: &Document) -> f32 {
        let mut score = 0.0_f32;
        let mut query_terms = ArrayVec::<&[u8], MAX_QUERY_TERMS>::new();
        let mut matches_found = 0_u32;

        // Split query into terms
        for term in query.split_whitespace() {
            if query_terms.try_push(term.as_bytes()).is_err() {
                break;
            }
        }

        let term_count = query_terms.len();
        if term_count == 0 {
            return 0.0;
        }

        // Score each term
        for term in query_terms {
            let mut term_score = 0.0;

            // Check filename match (60% weight)
            if Self::term_matches(term, doc.path.to_string_lossy().as_bytes()) {
                term_score += 0.6;
                matches_found += 1;
            }

            // Check content match (40% weight)
            if Self::term_matches(term, &doc.content) {
                term_score += 0.4;
                matches_found += 1;
            }

            score += term_score;
        }

        // Normalize score to 0-100% range
        if matches_found == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let term_count = u32::try_from(term_count).unwrap_or(1) as f32;
            (score / term_count * 100.0).min(100.0)
        }
    }

    /// Search for documents matching the query
    ///
    /// # Errors
    /// Returns error if result buffer is full
    pub fn search(&self, query: &str) -> Result<ArrayVec<SearchResult, MAX_RESULTS>> {
        let mut results = ArrayVec::new();
        let mut scores = ArrayVec::<(f32, usize), MAX_DOCUMENTS>::new();

        // Calculate scores and store document indices
        for (idx, doc) in self.documents.iter().enumerate() {
            let score = Self::calculate_score(query, doc);
            if score > 0.0 {
                let _ = scores.try_push((score, idx));
            }
        }

        // Sort by score in descending order
        scores
            .as_mut_slice()
            .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Take top results
        for (score, idx) in scores.iter().take(MAX_RESULTS) {
            results
                .try_push(SearchResult { path: self.documents[*idx].path.clone(), score: *score })
                .map_err(|_| Error::Search("Too many results".into()))?;
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;

    fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_engine_creation() {
        let _engine = SearchEngine::new();
    }

    #[test]
    fn test_add_and_search_document() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_file(
            &temp_dir,
            "test.txt",
            "This is a test document with some unique content",
        );

        // Add document
        let result = engine.add_document(&path, "This is a test document with some unique content");
        assert!(result.is_ok());

        // Search for content
        let results = engine.search("unique content").unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].path, path);
    }

    #[test]
    fn test_search_no_results() {
        let engine = SearchEngine::new();
        let results = engine.search("nonexistent").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_multiple_documents() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();

        // Add multiple documents
        for i in 0..5 {
            let path = create_test_file(
                &temp_dir,
                &format!("test_{i}.txt"),
                &format!("Document {i} with content"),
            );
            engine.add_document(&path, &format!("Document {i} with content")).unwrap();
        }

        // Search should find all documents
        let results = engine.search("Document").unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_result_limit() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();

        // Add documents up to MAX_DOCUMENTS
        for i in 0..MAX_DOCUMENTS {
            let path = create_test_file(&temp_dir, &format!("doc_{i}.txt"), "common content");
            engine.add_document(&path, "common content").unwrap();
        }

        // Search should return all documents since they all match
        let results = engine.search("common").unwrap();
        assert_eq!(results.len(), MAX_DOCUMENTS);

        // Try to add one more document - should fail
        let path = create_test_file(&temp_dir, "one_too_many.txt", "common content");
        assert!(engine.add_document(&path, "common content").is_err());
    }

    #[test]
    fn test_result_ranking() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();

        // Create documents with varying relevance
        let path1 = create_test_file(&temp_dir, "rust_guide.txt", "rust programming guide");
        let path2 = create_test_file(&temp_dir, "other.txt", "rust content");

        engine.add_document(&path1, "rust programming guide").unwrap();
        engine.add_document(&path2, "rust content").unwrap();

        // Document with "rust" in filename should score higher
        let results = engine.search("rust").unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].path, path1);
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_filename_match() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_file(&temp_dir, "important_doc.txt", "Some content");

        engine.add_document(&path, "Some content").unwrap();

        // Search should match filename
        let results = engine.search("important").unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].path, path);
    }

    #[test]
    fn test_case_insensitive() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_file(&temp_dir, "TEST.txt", "UPPERCASE CONTENT");

        engine.add_document(&path, "UPPERCASE CONTENT").unwrap();

        // Search should be case-insensitive
        let results = engine.search("uppercase").unwrap();
        assert!(!results.is_empty());
        let results2 = engine.search("UPPERCASE").unwrap();
        assert!(!results2.is_empty());
    }

    #[test]
    fn test_content_too_large() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();
        let large_content = "x".repeat(MAX_CONTENT_LENGTH + 1);
        let path = create_test_file(&temp_dir, "large.txt", &large_content);

        // Should fail to add document
        assert!(engine.add_document(&path, &large_content).is_err());
    }

    #[test]
    fn test_multiple_term_scoring() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();

        // Create documents with varying relevance
        let path1 = create_test_file(&temp_dir, "test1.txt", "rust programming guide");
        let path2 = create_test_file(&temp_dir, "rust_book.txt", "some other content");

        engine.add_document(&path1, "rust programming guide").unwrap();
        engine.add_document(&path2, "some other content").unwrap();

        // Document with both content and filename match should score higher
        let results = engine.search("rust").unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().join("test.idx");

        // Create and populate engine
        let mut engine = SearchEngine::new();
        let doc_path = create_test_file(&temp_dir, "test.txt", "test content");
        engine.add_document(&doc_path, "test content").unwrap();

        // Save index
        engine.save(&index_path).unwrap();

        // Load index
        let loaded = SearchEngine::load(&index_path).unwrap();

        // Verify loaded data
        assert_eq!(loaded.documents.len(), 1);
        assert_eq!(loaded.documents[0].path, doc_path);
        assert_eq!(String::from_utf8_lossy(&loaded.documents[0].content), "test content");
    }

    #[test]
    fn test_load_invalid_file() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_path = temp_dir.path().join("invalid.idx");

        // Create invalid file
        let mut file = File::create(&invalid_path).unwrap();
        file.write_all(b"INVALID").unwrap();

        // Attempt to load
        assert!(matches!(
            SearchEngine::load(&invalid_path),
            Err(Error::Search(msg)) if msg == "Invalid index file format"
        ));
    }

    #[test]
    fn test_load_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let missing_path = temp_dir.path().join("missing.idx");

        assert!(SearchEngine::load(&missing_path).is_err());
    }

    #[test]
    fn test_save_load_multiple_documents() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().join("multi.idx");

        // Create and populate engine
        let mut engine = SearchEngine::new();
        let mut paths = Vec::new();

        for i in 0..5 {
            let path =
                create_test_file(&temp_dir, &format!("doc_{i}.txt"), &format!("content {i}"));
            engine.add_document(&path, &format!("content {i}")).unwrap();
            paths.push(path);
        }

        // Save and load
        engine.save(&index_path).unwrap();
        let loaded = SearchEngine::load(&index_path).unwrap();

        // Verify all documents
        assert_eq!(loaded.documents.len(), 5);
        for (i, doc) in loaded.documents.iter().enumerate() {
            assert_eq!(doc.path, paths[i]);
            assert_eq!(String::from_utf8_lossy(&doc.content), format!("content {i}"));
        }
    }

    #[test]
    fn test_path_length_limit() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().join("long_path.idx");

        // Create engine with very long path
        let mut engine = SearchEngine::new();
        let long_name = "a".repeat(MAX_PATH_BYTES + 1);
        let doc_path = temp_dir.path().join(long_name);

        // Should fail to save
        engine.add_document(&doc_path, "content").unwrap();
        assert!(engine.save(&index_path).is_err());
    }
}
