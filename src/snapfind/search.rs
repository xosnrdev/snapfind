use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use arrayvec::ArrayVec;

use super::error::{SnapError, SnapResult};

pub const MAX_RESULTS: usize = 100;
pub const MAX_DOCUMENTS: usize = 100;
pub const MAX_CONTENT_LENGTH: usize = 1_000;
pub const MAX_TERM_LENGTH: usize = 50;
pub const MAX_PATH_BYTES: usize = 1024;
pub const MAGIC: [u8; 4] = *b"SNAP";
pub const VERSION: u8 = 1;
pub const MAX_PATTERNS: usize = 10;

pub const ERROR_INVALID_QUERY: i32 = 301;
pub const ERROR_INVALID_INDEX: i32 = 302;
pub const ERROR_TOO_MANY_DOCUMENTS: i32 = 303;
pub const ERROR_CONTENT_TOO_LARGE: i32 = 304;
pub const ERROR_PATH_TOO_LONG: i32 = 305;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: PathBuf,
    pub score: f32,
}

#[derive(Debug)]
pub struct Document {
    pub path: PathBuf,
    pub content: ArrayVec<u8, MAX_CONTENT_LENGTH>,
}

#[derive(Debug)]
struct GlobMatcher {
    patterns: ArrayVec<globset::GlobMatcher, MAX_PATTERNS>,
}

impl GlobMatcher {
    fn new(pattern: &str) -> SnapResult<Self> {
        assert!(!pattern.is_empty(), "Pattern must not be empty");
        assert!(pattern.len() <= MAX_TERM_LENGTH, "Pattern too long");

        let mut matcher = Self {
            patterns: ArrayVec::new(),
        };

        for part in pattern.split_whitespace() {
            let pattern_str = if !part.starts_with('*') && part.contains('*') {
                format!("*{part}")
            } else {
                part.to_string()
            };

            let glob = globset::GlobBuilder::new(&pattern_str)
                .case_insensitive(true)
                .literal_separator(false)
                .build()
                .map_err(|e| {
                    SnapError::with_code(format!("Invalid pattern: {e}"), ERROR_INVALID_QUERY)
                })?;

            matcher
                .patterns
                .try_push(glob.compile_matcher())
                .map_err(|_| SnapError::with_code("Too many pattern parts", ERROR_INVALID_QUERY))?;
        }

        assert!(
            !matcher.patterns.is_empty(),
            "Must have at least one pattern"
        );

        Ok(matcher)
    }

    fn is_match(&self, path: &Path) -> bool {
        assert!(path.as_os_str().len() <= MAX_PATH_BYTES, "Path too long");

        path.to_str()
            .is_some_and(|path_str| self.patterns.iter().any(|glob| glob.is_match(path_str)))
    }
}

#[derive(Debug)]
pub struct SearchEngine {
    documents: Box<ArrayVec<Document, MAX_DOCUMENTS>>,
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchEngine {
    #[must_use = "SearchEngine must be used to store and search documents"]
    pub fn new() -> Self {
        Self {
            documents: Box::new(ArrayVec::new()),
        }
    }

    pub fn load(path: &Path) -> SnapResult<Self> {
        let mut file = File::open(path).map_err(|e| {
            SnapError::with_code(format!("Failed to open index: {e}"), ERROR_INVALID_INDEX)
        })?;

        let mut magic = [0u8; 4];
        file.read_exact(&mut magic).map_err(|e| {
            SnapError::with_code(format!("Failed to read magic: {e}"), ERROR_INVALID_INDEX)
        })?;
        if magic != MAGIC {
            return Err(anyhow::Error::from(SnapError::with_code(
                "Invalid index file format",
                ERROR_INVALID_INDEX,
            )));
        }

        let mut version = [0u8; 1];
        file.read_exact(&mut version).map_err(|e| {
            SnapError::with_code(format!("Failed to read version: {e}"), ERROR_INVALID_INDEX)
        })?;
        if version[0] != VERSION {
            return Err(anyhow::Error::from(SnapError::with_code(
                format!("Unsupported index version: {}", version[0]),
                ERROR_INVALID_INDEX,
            )));
        }

        let mut ndocs = [0u8; 4];
        file.read_exact(&mut ndocs).map_err(|e| {
            SnapError::with_code(
                format!("Failed to read document count: {e}"),
                ERROR_INVALID_INDEX,
            )
        })?;
        let ndocs = u32::from_le_bytes(ndocs) as usize;
        if ndocs > MAX_DOCUMENTS {
            return Err(anyhow::Error::from(SnapError::with_code(
                "Too many documents in index",
                ERROR_TOO_MANY_DOCUMENTS,
            )));
        }

        let mut engine = Self::new();

        for _ in 0..ndocs {
            let mut path_len = [0u8; 2];
            file.read_exact(&mut path_len).map_err(|e| {
                SnapError::with_code(
                    format!("Failed to read path length: {e}"),
                    ERROR_INVALID_INDEX,
                )
            })?;
            let path_len = u16::from_le_bytes(path_len) as usize;
            if path_len > MAX_PATH_BYTES {
                return Err(anyhow::Error::from(SnapError::with_code(
                    "Path too long",
                    ERROR_PATH_TOO_LONG,
                )));
            }

            let mut path_buf = ArrayVec::<u8, MAX_PATH_BYTES>::new();
            for _ in 0..path_len {
                let mut byte = [0u8; 1];
                file.read_exact(&mut byte).map_err(|e| {
                    SnapError::with_code(format!("Failed to read path: {e}"), ERROR_INVALID_INDEX)
                })?;
                path_buf.try_push(byte[0]).map_err(|_| {
                    anyhow::Error::from(SnapError::with_code("Path too long", ERROR_PATH_TOO_LONG))
                })?;
            }

            let mut content_len = [0u8; 2];
            file.read_exact(&mut content_len).map_err(|e| {
                SnapError::with_code(
                    format!("Failed to read content length: {e}"),
                    ERROR_INVALID_INDEX,
                )
            })?;
            let content_len = u16::from_le_bytes(content_len) as usize;
            if content_len > MAX_CONTENT_LENGTH {
                return Err(anyhow::Error::from(SnapError::with_code(
                    "Content too large",
                    ERROR_CONTENT_TOO_LARGE,
                )));
            }

            let mut content = ArrayVec::new();
            for _ in 0..content_len {
                let mut byte = [0u8; 1];
                file.read_exact(&mut byte).map_err(|e| {
                    SnapError::with_code(
                        format!("Failed to read content: {e}"),
                        ERROR_INVALID_INDEX,
                    )
                })?;
                content.try_push(byte[0]).map_err(|_| {
                    anyhow::Error::from(SnapError::with_code(
                        "Content too large",
                        ERROR_CONTENT_TOO_LARGE,
                    ))
                })?;
            }

            let path_str = String::from_utf8_lossy(&path_buf).into_owned();
            let path = PathBuf::from(path_str);
            engine
                .documents
                .try_push(Document { path, content })
                .map_err(|_| {
                    anyhow::Error::from(SnapError::with_code(
                        "Too many documents",
                        ERROR_TOO_MANY_DOCUMENTS,
                    ))
                })?;
        }

        Ok(engine)
    }

    pub fn save(&self, path: &Path) -> SnapResult<()> {
        let mut file = File::create(path).map_err(|e| {
            SnapError::with_code(format!("Failed to create index: {e}"), ERROR_INVALID_INDEX)
        })?;

        file.write_all(&MAGIC).map_err(|e| {
            SnapError::with_code(format!("Failed to write magic: {e}"), ERROR_INVALID_INDEX)
        })?;
        file.write_all(&[VERSION]).map_err(|e| {
            SnapError::with_code(format!("Failed to write version: {e}"), ERROR_INVALID_INDEX)
        })?;

        let ndocs = u32::try_from(self.documents.len()).map_err(|_| {
            SnapError::with_code(
                "Too many documents for index format",
                ERROR_TOO_MANY_DOCUMENTS,
            )
        })?;
        file.write_all(&ndocs.to_le_bytes()).map_err(|e| {
            SnapError::with_code(
                format!("Failed to write document count: {e}"),
                ERROR_INVALID_INDEX,
            )
        })?;

        for doc in self.documents.iter() {
            let path_str = doc.path.to_string_lossy();
            let path_bytes = path_str.as_bytes();
            if path_bytes.len() > MAX_PATH_BYTES {
                return Err(anyhow::Error::from(SnapError::with_code(
                    "Path too long",
                    ERROR_PATH_TOO_LONG,
                )));
            }

            let path_len = u16::try_from(path_bytes.len()).map_err(|_| {
                anyhow::Error::from(SnapError::with_code(
                    "Path too long for index format",
                    ERROR_PATH_TOO_LONG,
                ))
            })?;
            file.write_all(&path_len.to_le_bytes()).map_err(|e| {
                anyhow::Error::from(SnapError::with_code(
                    format!("Failed to write path length: {e}"),
                    ERROR_INVALID_INDEX,
                ))
            })?;
            file.write_all(path_bytes).map_err(|e| {
                anyhow::Error::from(SnapError::with_code(
                    format!("Failed to write path: {e}"),
                    ERROR_INVALID_INDEX,
                ))
            })?;

            let content_len = u16::try_from(doc.content.len()).map_err(|_| {
                anyhow::Error::from(SnapError::with_code(
                    "Content too large for index format",
                    ERROR_CONTENT_TOO_LARGE,
                ))
            })?;
            file.write_all(&content_len.to_le_bytes()).map_err(|e| {
                anyhow::Error::from(SnapError::with_code(
                    format!("Failed to write content length: {e}"),
                    ERROR_INVALID_INDEX,
                ))
            })?;
            file.write_all(&doc.content).map_err(|e| {
                anyhow::Error::from(SnapError::with_code(
                    format!("Failed to write content: {e}"),
                    ERROR_INVALID_INDEX,
                ))
            })?;
        }

        Ok(())
    }

    pub fn add_document(&mut self, path: &Path, content: &str) -> SnapResult<()> {
        let mut doc_content = ArrayVec::new();
        for &b in content.as_bytes() {
            doc_content.try_push(b).map_err(|_| {
                anyhow::Error::from(SnapError::with_code(
                    "Content too large",
                    ERROR_CONTENT_TOO_LARGE,
                ))
            })?;
        }

        self.documents
            .try_push(Document {
                path: path.to_path_buf(),
                content: doc_content,
            })
            .map_err(|_| {
                anyhow::Error::from(SnapError::with_code(
                    "Too many documents",
                    ERROR_TOO_MANY_DOCUMENTS,
                ))
            })?;

        Ok(())
    }

    #[must_use]
    pub fn term_matches(term: &[u8], content: &[u8]) -> bool {
        if term.is_empty() || content.is_empty() || term.len() > content.len() {
            return false;
        }

        let mut term_lower = ArrayVec::<u8, MAX_TERM_LENGTH>::new();
        for &b in term {
            if term_lower.try_push(b.to_ascii_lowercase()).is_err() {
                return false;
            }
        }

        let mut content_lower = ArrayVec::<u8, MAX_CONTENT_LENGTH>::new();
        for &b in content {
            if content_lower.try_push(b.to_ascii_lowercase()).is_err() {
                return false;
            }
        }

        let content_lower = &content_lower;
        let term_lower = &term_lower;

        for i in 0..=content_lower.len().saturating_sub(term_lower.len()) {
            let is_start = i == 0 || !content_lower[i - 1].is_ascii_alphanumeric();
            let is_end = i + term_lower.len() == content_lower.len()
                || !content_lower[i + term_lower.len()].is_ascii_alphanumeric();

            if is_start && is_end {
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

    #[must_use]
    pub fn calculate_score(query: &str, doc: &Document) -> f32 {
        let mut score = 0.0_f32;
        let mut query_terms = ArrayVec::<&[u8], 10>::new();
        let mut matches_found = 0_u32;

        for term in query.split_whitespace() {
            if query_terms.try_push(term.as_bytes()).is_err() {
                break;
            }
        }

        let term_count = query_terms.len();
        if term_count == 0 {
            return 0.0;
        }

        for term in query_terms {
            let mut term_score = 0.0;

            if Self::term_matches(term, doc.path.to_string_lossy().as_bytes()) {
                term_score += 0.6;
                matches_found += 1;
            }

            if Self::term_matches(term, &doc.content) {
                term_score += 0.4;
                matches_found += 1;
            }

            score += term_score;
        }

        if matches_found == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let term_count = u32::try_from(term_count).unwrap_or(1) as f32;
            (score / term_count * 100.0).min(100.0)
        }
    }

    pub fn search(&self, query: &str) -> SnapResult<ArrayVec<SearchResult, MAX_RESULTS>> {
        validate_query(query)?;

        let mut results = ArrayVec::new();
        let mut scores = ArrayVec::<(f32, usize), MAX_DOCUMENTS>::new();

        let glob_matcher = GlobMatcher::new(query)?;
        let is_glob_query = query.contains('*');

        for (idx, doc) in self.documents.iter().enumerate() {
            let score = if is_glob_query {
                if glob_matcher.is_match(&doc.path) {
                    100.0
                } else {
                    0.0
                }
            } else {
                let base_score = Self::calculate_score(query, doc);
                if glob_matcher.is_match(&doc.path) {
                    (base_score * 1.5).min(100.0)
                } else {
                    base_score
                }
            };

            if score > 0.0 {
                scores.try_push((score, idx)).map_err(|_| {
                    anyhow::Error::from(SnapError::with_code(
                        "Too many matching documents",
                        ERROR_TOO_MANY_DOCUMENTS,
                    ))
                })?;
            }
        }

        assert!(scores.len() <= MAX_DOCUMENTS, "Score buffer overflow");

        scores
            .as_mut_slice()
            .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        for (score, idx) in scores.iter().take(MAX_RESULTS) {
            results
                .try_push(SearchResult {
                    path: self.documents[*idx].path.clone(),
                    score: *score,
                })
                .map_err(|_| {
                    anyhow::Error::from(SnapError::with_code(
                        "Too many results",
                        ERROR_TOO_MANY_DOCUMENTS,
                    ))
                })?;
        }

        assert!(results.len() <= MAX_RESULTS, "Result buffer overflow");

        Ok(results)
    }
}

pub fn validate_query(query: &str) -> SnapResult<()> {
    if query.is_empty() {
        return Err(anyhow::Error::from(SnapError::with_code(
            "Query must not be empty",
            ERROR_INVALID_QUERY,
        )));
    }

    if query.len() > MAX_TERM_LENGTH {
        return Err(anyhow::Error::from(SnapError::with_code(
            "Query too long",
            ERROR_INVALID_QUERY,
        )));
    }

    if query.contains('\0') || !query.chars().all(|c| c.is_ascii() || c.is_whitespace()) {
        return Err(anyhow::Error::from(SnapError::with_code(
            "Query contains invalid characters",
            ERROR_INVALID_QUERY,
        )));
    }

    Ok(())
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

        let result = engine.add_document(&path, "This is a test document with some unique content");
        assert!(result.is_ok());

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

        for i in 0..5 {
            let path = create_test_file(
                &temp_dir,
                &format!("test_{i}.txt"),
                &format!("Document {i} with content"),
            );
            engine
                .add_document(&path, &format!("Document {i} with content"))
                .unwrap();
        }

        let results = engine.search("Document").unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_result_limit() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();

        for i in 0..MAX_DOCUMENTS {
            let path = create_test_file(&temp_dir, &format!("doc_{i}.txt"), "common content");
            engine.add_document(&path, "common content").unwrap();
        }

        let results = engine.search("common").unwrap();
        assert_eq!(results.len(), MAX_DOCUMENTS);

        let path = create_test_file(&temp_dir, "one_too_many.txt", "common content");
        assert!(engine.add_document(&path, "common content").is_err());
    }

    #[test]
    fn test_result_ranking() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();

        let path1 = create_test_file(&temp_dir, "rust_guide.txt", "rust programming guide");
        let path2 = create_test_file(&temp_dir, "other.txt", "rust content");

        engine
            .add_document(&path1, "rust programming guide")
            .unwrap();
        engine.add_document(&path2, "rust content").unwrap();

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
        assert!(engine.add_document(&path, &large_content).is_err());
    }

    #[test]
    fn test_multiple_term_scoring() {
        let mut engine = SearchEngine::new();
        let temp_dir = TempDir::new().unwrap();

        let path1 = create_test_file(&temp_dir, "test1.txt", "rust programming guide");
        let path2 = create_test_file(&temp_dir, "rust_book.txt", "some other content");

        engine
            .add_document(&path1, "rust programming guide")
            .unwrap();
        engine.add_document(&path2, "some other content").unwrap();

        let results = engine.search("rust").unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().join("test.idx");

        let mut engine = SearchEngine::new();
        let doc_path = create_test_file(&temp_dir, "test.txt", "test content");
        engine.add_document(&doc_path, "test content").unwrap();

        engine.save(&index_path).unwrap();

        let loaded = SearchEngine::load(&index_path).unwrap();

        assert_eq!(loaded.documents.len(), 1);
        assert_eq!(loaded.documents[0].path, doc_path);
        assert_eq!(
            String::from_utf8_lossy(&loaded.documents[0].content),
            "test content"
        );
    }

    #[test]
    fn test_load_invalid_file() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_path = temp_dir.path().join("invalid.idx");
        File::create(&invalid_path).unwrap();

        let result = SearchEngine::load(&invalid_path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("magic") || err.contains("index") || err.contains("Invalid"),
            "Error should mention file format issue. Got: {err}"
        );
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

        let mut engine = SearchEngine::new();
        let mut paths = Vec::new();

        for i in 0..5 {
            let path =
                create_test_file(&temp_dir, &format!("doc_{i}.txt"), &format!("content {i}"));
            engine.add_document(&path, &format!("content {i}")).unwrap();
            paths.push(path);
        }

        engine.save(&index_path).unwrap();
        let loaded = SearchEngine::load(&index_path).unwrap();

        assert_eq!(loaded.documents.len(), 5);
        for (i, doc) in loaded.documents.iter().enumerate() {
            assert_eq!(doc.path, paths[i]);
            assert_eq!(
                String::from_utf8_lossy(&doc.content),
                format!("content {i}")
            );
        }
    }

    #[test]
    fn test_path_length_limit() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().join("long_path.idx");

        let mut engine = SearchEngine::new();
        let long_name = "a".repeat(MAX_PATH_BYTES + 1);
        let doc_path = temp_dir.path().join(long_name);

        engine.add_document(&doc_path, "content").unwrap();
        assert!(engine.save(&index_path).is_err());
    }

    #[test]
    fn test_validate_query() {
        assert!(validate_query("test").is_ok());
        assert!(validate_query("test.txt").is_ok());
        assert!(validate_query("*.txt").is_ok());
        assert!(validate_query("").is_err());

        let long_query = "a".repeat(MAX_TERM_LENGTH + 1);
        assert!(validate_query(&long_query).is_err());

        assert!(validate_query("test\0file").is_err());
    }
}
