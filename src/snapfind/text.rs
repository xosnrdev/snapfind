pub const TEXT_SAMPLE_SIZE: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMimeType {
    Plain,
    Markdown,
    Source,
    Config,
    Unknown,
}

#[derive(Debug)]
pub struct TextValidation {
    confidence: u8,
    #[allow(dead_code)]
    encoding: TextEncoding,
    mime_type: TextMimeType,
}

#[derive(Debug)]
pub struct TextStats {
    null_bytes: u16,
    control_chars: u16,
    utf8_errors: u16,
    line_breaks: u16,
    ascii_ratio: u8,
}

#[derive(Debug)]
pub struct TextDetector {
    stats: TextStats,
    sample_buf: [u8; TEXT_SAMPLE_SIZE],
}

impl TextValidation {
    #[must_use]
    pub const fn binary() -> Self {
        Self {
            confidence: 0,
            encoding: TextEncoding::Unknown,
            mime_type: TextMimeType::Unknown,
        }
    }

    #[must_use]
    pub const fn is_valid_text(&self) -> bool {
        self.confidence >= 50
    }

    #[must_use]
    pub const fn confidence(&self) -> u8 {
        self.confidence
    }

    #[cfg(test)]
    #[must_use]
    pub const fn encoding(&self) -> TextEncoding {
        self.encoding
    }

    #[must_use]
    pub const fn mime_type(&self) -> TextMimeType {
        self.mime_type
    }
}

impl TextStats {
    const fn new() -> Self {
        Self {
            null_bytes: 0,
            control_chars: 0,
            utf8_errors: 0,
            line_breaks: 0,
            ascii_ratio: 0,
        }
    }

    const fn reset(&mut self) {
        *self = Self::new();
    }

    fn update(&mut self, byte: u8) {
        if byte == 0 {
            assert!(self.null_bytes < u16::try_from(TEXT_SAMPLE_SIZE).unwrap());
            self.null_bytes += 1;
        }

        if byte < 32 && !matches!(byte, b'\n' | b'\r' | b'\t') {
            assert!(self.control_chars < u16::try_from(TEXT_SAMPLE_SIZE).unwrap());
            self.control_chars += 1;
        }

        if byte == b'\n' {
            assert!(self.line_breaks < u16::try_from(TEXT_SAMPLE_SIZE).unwrap());
            self.line_breaks += 1;
        }

        if byte < 128 {
            self.ascii_ratio =
                u8::try_from((u16::from(self.ascii_ratio) * 99 + 100) / 100).unwrap();
        } else {
            self.ascii_ratio = u8::try_from((u16::from(self.ascii_ratio) * 99) / 100).unwrap();
        }
        assert!(self.ascii_ratio <= 100);
    }
}

impl Default for TextDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl TextDetector {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            stats: TextStats::new(),
            sample_buf: [0; TEXT_SAMPLE_SIZE],
        }
    }

    #[must_use]
    pub fn validate(&mut self, content: &[u8]) -> TextValidation {
        if !Self::check_basic_validity(content) {
            return TextValidation::binary();
        }

        if !self.analyze_content(content) {
            return TextValidation::binary();
        }

        self.determine_result()
    }

    const fn check_basic_validity(content: &[u8]) -> bool {
        if content.is_empty() {
            return false;
        }

        if content.len() > TEXT_SAMPLE_SIZE * 1024 {
            return false;
        }

        true
    }

    fn analyze_content(&mut self, content: &[u8]) -> bool {
        self.stats.reset();

        let sample_size = content.len().min(TEXT_SAMPLE_SIZE);
        self.sample_buf[..sample_size].copy_from_slice(&content[..sample_size]);

        for &byte in &content[..sample_size] {
            self.stats.update(byte);
        }

        if self.stats.null_bytes > u16::try_from(sample_size).unwrap_or(u16::MAX) / 10 {
            return false;
        }

        let ascii_count = content[..sample_size].iter().filter(|&&b| b < 128).count();
        self.stats.ascii_ratio = u8::try_from((ascii_count * 100) / sample_size).unwrap();

        if let Err(e) = std::str::from_utf8(&content[..sample_size]) {
            self.stats.utf8_errors = u16::try_from(e.valid_up_to()).unwrap();
        }

        true
    }

    fn determine_result(&self) -> TextValidation {
        if self.is_binary_header() || self.stats.null_bytes > 0 {
            return TextValidation::binary();
        }

        let mut confidence = 100_u8;

        if self.stats.control_chars > 0 {
            confidence =
                confidence.saturating_sub(u8::try_from(self.stats.control_chars).unwrap_or(100));
        }

        if self.stats.utf8_errors > 0 {
            confidence =
                confidence.saturating_sub(u8::try_from(self.stats.utf8_errors * 10).unwrap_or(100));
        }

        if self.stats.line_breaks < 2 {
            confidence = confidence.saturating_sub(20);
        }

        if self.stats.ascii_ratio < 90 {
            confidence = confidence.saturating_sub(90_u8.saturating_sub(self.stats.ascii_ratio));
        }

        let mime_type = if self.stats.line_breaks == 0 {
            TextMimeType::Plain
        } else if self.sample_buf.starts_with(b"#!") || self.sample_buf.starts_with(b"<?") {
            TextMimeType::Source
        } else if self.sample_buf.starts_with(b"[")
            || (self.sample_buf.starts_with(b"# ") && self.sample_buf.contains(&b'['))
        {
            TextMimeType::Config
        } else if self.sample_buf.starts_with(b"# ")
            || self.sample_buf.starts_with(b"## ")
            || (self.sample_buf.contains(&b'#')
                && (self.sample_buf.contains(&b'*')
                    || self.sample_buf.contains(&b'-')
                    || self.sample_buf.contains(&b'[')
                    || self.sample_buf.contains(&b'`')))
        {
            TextMimeType::Markdown
        } else if self.sample_buf.contains(&b'{')
            || self.sample_buf.contains(&b'}')
            || self.sample_buf.contains(&b'=')
            || self.sample_buf.contains(&b';')
        {
            TextMimeType::Source
        } else {
            TextMimeType::Plain
        };

        TextValidation {
            confidence: confidence.min(100),
            encoding: TextEncoding::Utf8,
            mime_type,
        }
    }

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
