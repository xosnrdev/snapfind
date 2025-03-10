//! Text file detection and validation

/// Maximum sample size for text validation
pub const TEXT_SAMPLE_SIZE: usize = 512;

/// Text encoding types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    /// UTF-8 encoded text
    Utf8,
    /// Unknown or invalid encoding
    Unknown,
}

/// MIME type categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMimeType {
    /// text/plain
    Plain,
    /// text/markdown
    Markdown,
    /// text/x-*
    Source,
    /// text/x-config
    Config,
    /// application/octet-stream
    Unknown,
}

/// Text validation result
#[derive(Debug)]
pub struct TextValidation {
    /// Confidence score (0-100)
    confidence: u8,
    /// Detected encoding
    #[allow(dead_code)]
    encoding:   TextEncoding,
    /// Detected MIME type
    mime_type:  TextMimeType,
}

/// Statistical metrics for text validation
#[derive(Debug)]
pub struct TextStats {
    /// Number of null bytes found
    null_bytes:    u16,
    /// Number of control characters found
    control_chars: u16,
    /// Number of UTF-8 encoding errors
    utf8_errors:   u16,
    /// Number of line breaks found
    line_breaks:   u16,
    /// Percentage of ASCII bytes (0-100)
    ascii_ratio:   u8,
}

/// Text detector with pre-allocated buffers
#[derive(Debug)]
pub struct TextDetector {
    /// Statistical metrics
    stats:      TextStats,
    /// Sample buffer
    sample_buf: [u8; TEXT_SAMPLE_SIZE],
}

impl TextValidation {
    /// Creates a binary validation result
    pub const fn binary() -> Self {
        Self { confidence: 0, encoding: TextEncoding::Unknown, mime_type: TextMimeType::Unknown }
    }

    /// Returns true if the content is valid text
    pub const fn is_valid_text(&self) -> bool {
        self.confidence >= 50
    }

    /// Returns the confidence score (0-100)
    pub const fn confidence(&self) -> u8 {
        self.confidence
    }

    /// Returns the detected encoding
    #[cfg(test)]
    pub const fn encoding(&self) -> TextEncoding {
        self.encoding
    }

    /// Returns the detected MIME type
    pub const fn mime_type(&self) -> TextMimeType {
        self.mime_type
    }
}

impl TextStats {
    /// Creates a new stats tracker
    const fn new() -> Self {
        Self {
            null_bytes:    0,
            control_chars: 0,
            utf8_errors:   0,
            line_breaks:   0,
            ascii_ratio:   0,
        }
    }

    /// Resets all statistics
    const fn reset(&mut self) {
        *self = Self::new();
    }

    /// Updates statistics for a single byte
    fn update(&mut self, byte: u8) {
        // Track null bytes
        if byte == 0 {
            assert!(self.null_bytes < u16::try_from(TEXT_SAMPLE_SIZE).unwrap());
            self.null_bytes += 1;
        }

        // Track control characters
        if byte < 32 && !matches!(byte, b'\n' | b'\r' | b'\t') {
            assert!(self.control_chars < u16::try_from(TEXT_SAMPLE_SIZE).unwrap());
            self.control_chars += 1;
        }

        // Track line breaks
        if byte == b'\n' {
            assert!(self.line_breaks < u16::try_from(TEXT_SAMPLE_SIZE).unwrap());
            self.line_breaks += 1;
        }

        // Update ASCII ratio
        if byte < 128 {
            self.ascii_ratio =
                u8::try_from((u16::from(self.ascii_ratio) * 99 + 100) / 100).unwrap();
        } else {
            self.ascii_ratio = u8::try_from((u16::from(self.ascii_ratio) * 99) / 100).unwrap();
        }
        assert!(self.ascii_ratio <= 100);
    }
}

impl TextDetector {
    /// Creates a new detector with pre-allocated buffers
    #[must_use]
    pub const fn new() -> Self {
        Self { stats: TextStats::new(), sample_buf: [0; TEXT_SAMPLE_SIZE] }
    }

    /// Validates text content
    #[must_use]
    pub fn validate(&mut self, content: &[u8]) -> TextValidation {
        // Basic validity check
        if !Self::check_basic_validity(content) {
            return TextValidation::binary();
        }

        // Analyze content in detail
        if !self.analyze_content(content) {
            return TextValidation::binary();
        }

        // Determine final result
        self.determine_result()
    }

    /// Checks basic validity of content
    const fn check_basic_validity(content: &[u8]) -> bool {
        // Empty content is not valid
        if content.is_empty() {
            return false;
        }

        // Content must not be too large
        if content.len() > TEXT_SAMPLE_SIZE * 1024 {
            return false;
        }

        true
    }

    /// Analyzes content for text validity
    fn analyze_content(&mut self, content: &[u8]) -> bool {
        // Reset stats
        self.stats.reset();

        // Sample content
        let sample_size = content.len().min(TEXT_SAMPLE_SIZE);
        self.sample_buf[..sample_size].copy_from_slice(&content[..sample_size]);

        // Analyze each byte
        for &byte in &content[..sample_size] {
            self.stats.update(byte);
        }

        // Early return if too many null bytes
        if self.stats.null_bytes > u16::try_from(sample_size).unwrap_or(u16::MAX) / 10 {
            return false;
        }

        // Update ASCII ratio based on final counts
        let ascii_count = content[..sample_size].iter().filter(|&&b| b < 128).count();
        self.stats.ascii_ratio = u8::try_from((ascii_count * 100) / sample_size).unwrap();

        // Check for UTF-8 validity
        if let Err(e) = std::str::from_utf8(&content[..sample_size]) {
            self.stats.utf8_errors = u16::try_from(e.valid_up_to()).unwrap();
        }

        true
    }

    /// Determines final validation result
    fn determine_result(&self) -> TextValidation {
        // Calculate confidence score
        let mut confidence: u8 = 100;

        // Early return for likely binary content
        if self.is_binary_header() || self.stats.null_bytes > 10 {
            return TextValidation::binary();
        }

        // Penalize for problematic content
        confidence =
            confidence.saturating_sub(u8::try_from(self.stats.null_bytes).unwrap_or(255) * 10);
        confidence =
            confidence.saturating_sub(u8::try_from(self.stats.control_chars).unwrap_or(255) * 5);
        confidence =
            confidence.saturating_sub(u8::try_from(self.stats.utf8_errors).unwrap_or(255) * 20);

        // Heavy penalty for low ASCII ratio
        if self.stats.ascii_ratio < 50 {
            confidence = confidence.saturating_sub(50);
        }

        // Boost for good indicators
        if self.stats.line_breaks > 0 {
            confidence = confidence.saturating_add(10);
        }
        if self.stats.ascii_ratio > 90 {
            confidence = confidence.saturating_add(30);
        }

        // Determine encoding based on content analysis
        let encoding = if confidence < 50 {
            TextEncoding::Unknown
        } else if self.stats.utf8_errors == 0 {
            TextEncoding::Utf8
        } else {
            TextEncoding::Unknown
        };

        // Determine MIME type from content patterns
        let mime_type = if confidence < 50 {
            TextMimeType::Unknown
        } else {
            let sample = std::str::from_utf8(&self.sample_buf).unwrap_or("");

            // Check for markdown indicators
            if sample.starts_with('#') || sample.contains("\n#") || sample.contains("* ") {
                TextMimeType::Markdown
            }
            // Check for source code indicators
            else if sample.contains("fn ")
                || sample.contains("pub ")
                || sample.contains("class ")
                || sample.contains("def ")
            {
                TextMimeType::Source
            }
            // Check for config file indicators
            else if (sample.contains('=') || sample.contains(':'))
                && sample.contains('[')
                && sample.contains(']')
            {
                TextMimeType::Config
            }
            // Default to plain text
            else {
                TextMimeType::Plain
            }
        };

        TextValidation { confidence, encoding, mime_type }
    }

    /// Checks for common binary file headers
    fn is_binary_header(&self) -> bool {
        let sample = &self.sample_buf[..4];
        matches!(sample, b"PK\x03\x04" | b"\x7FELF" | b"\x89PNG")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_content() {
        let mut detector = TextDetector::new();
        let result = detector.validate(&[]);
        assert!(!result.is_valid_text());
        assert_eq!(result.confidence(), 0);
        assert_eq!(result.encoding(), TextEncoding::Unknown);
        assert_eq!(result.mime_type(), TextMimeType::Unknown);
    }

    #[test]
    fn test_plain_text() {
        let mut detector = TextDetector::new();
        let content = b"Hello, world!\nThis is a test.\n";
        let result = detector.validate(content);
        assert!(result.is_valid_text());
        assert_eq!(result.encoding(), TextEncoding::Utf8);
        assert_eq!(result.mime_type(), TextMimeType::Plain);
    }

    #[test]
    fn test_markdown_text() {
        let mut detector = TextDetector::new();
        let content = b"# Heading\n\n* List item\n* Another item\n";
        let result = detector.validate(content);
        assert!(result.is_valid_text());
        assert_eq!(result.encoding(), TextEncoding::Utf8);
        assert_eq!(result.mime_type(), TextMimeType::Markdown);
    }

    #[test]
    fn test_source_code() {
        let mut detector = TextDetector::new();
        let content = b"fn main() {\n    println!(\"Hello\");\n}\n";
        let result = detector.validate(content);
        assert!(result.is_valid_text());
        assert_eq!(result.encoding(), TextEncoding::Utf8);
        assert_eq!(result.mime_type(), TextMimeType::Source);
    }

    #[test]
    fn test_config_file() {
        let mut detector = TextDetector::new();
        let content = b"[section]\nkey=value\n";
        let result = detector.validate(content);
        assert!(result.is_valid_text());
        assert_eq!(result.encoding(), TextEncoding::Utf8);
        assert_eq!(result.mime_type(), TextMimeType::Config);
    }

    #[test]
    fn test_binary_content() {
        let mut detector = TextDetector::new();
        let content = b"PK\x03\x04\x00\x00\x00\x00";
        let result = detector.validate(content);
        assert!(!result.is_valid_text());
        assert_eq!(result.confidence(), 0);
        assert_eq!(result.encoding(), TextEncoding::Unknown);
        assert_eq!(result.mime_type(), TextMimeType::Unknown);
    }

    #[test]
    fn test_high_confidence_text() {
        let mut detector = TextDetector::new();
        let content = b"This is a very normal text file.\nIt has multiple lines.\nAll ASCII.";
        let result = detector.validate(content);
        assert!(result.confidence() > 90);
        assert_eq!(result.encoding(), TextEncoding::Utf8);
    }

    #[test]
    fn test_low_confidence_text() {
        let mut detector = TextDetector::new();
        let mut content = Vec::new();
        for i in 0..100 {
            content.push(u8::try_from(i).unwrap());
        }
        let result = detector.validate(&content);
        assert!(result.confidence() < 50);
        assert_eq!(result.encoding(), TextEncoding::Unknown);
    }

    #[test]
    fn test_ascii_text() {
        let mut detector = TextDetector::new();
        let content = b"Pure ASCII content\nNo special chars\n";
        let result = detector.validate(content);
        assert!(result.is_valid_text());
        assert_eq!(result.encoding(), TextEncoding::Utf8);
        assert!(result.confidence() > 90);
    }
}
