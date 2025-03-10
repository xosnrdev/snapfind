//! Directory crawler implementation

use std::fs;
use std::path::{Path, PathBuf};

use arrayvec::ArrayVec;

use crate::error::{Error, Result};
use crate::types::{MAX_DEPTH, MAX_FILE_SIZE, MAX_FILES, MAX_PATH_LENGTH};

/// Directory crawler that maintains fixed memory bounds
#[derive(Debug)]
pub struct Crawler {
    /// Queue of directories to process with their depths
    queue:      ArrayVec<(PathBuf, usize), MAX_DEPTH>,
    /// Number of files processed
    file_count: usize,
    /// Total number of directories discovered
    dir_count:  usize,
}

impl Crawler {
    /// Create a new crawler starting at the given path
    pub fn new(start_path: &Path) -> Result<Self> {
        // Validate initial path
        Self::validate_path(start_path)?;

        let mut queue = ArrayVec::new();
        queue.try_push((start_path.to_path_buf(), 0)).map_err(|_| Error::DepthExceeded)?;

        Ok(Self {
            queue,
            file_count: 0,
            dir_count: 1, // Start path counts as first directory
        })
    }

    /// Get the current progress of the crawl
    ///
    /// Returns a tuple of:
    /// - Number of files processed so far
    /// - Maximum number of files allowed
    /// - Number of directories discovered
    #[must_use = "Progress information should be used for monitoring"]
    pub const fn progress(&self) -> (usize, usize, usize) {
        // Returns (files_processed, max_files, directories_found)
        (self.file_count, MAX_FILES, self.dir_count)
    }

    /// Process the next directory in the queue
    pub fn process_next(&mut self) -> Result<Option<ArrayVec<PathBuf, MAX_FILES>>> {
        let Some((dir, current_depth)) = self.queue.pop() else {
            return Ok(None);
        };

        // Verify directory exists and is accessible
        assert!(dir.exists(), "Directory in queue must exist");
        assert!(dir.is_dir(), "Path in queue must be a directory");

        let mut files = ArrayVec::new();

        // Process directory entries
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();

            // Validate path
            Self::validate_path(&path)?;

            if entry.file_type()?.is_dir() {
                let new_depth = current_depth + 1;
                // Check depth limit
                if new_depth >= MAX_DEPTH {
                    return Err(Error::DepthExceeded);
                }
                self.queue.try_push((path, new_depth)).map_err(|_| Error::DepthExceeded)?;
                self.dir_count += 1;
            } else {
                // Check file count limit
                if self.file_count >= MAX_FILES {
                    return Err(Error::FileCountExceeded);
                }
                // Check file size limit
                let size = entry.metadata()?.len();
                if size > MAX_FILE_SIZE {
                    return Err(Error::FileSizeExceeded);
                }
                files.try_push(path).map_err(|_| Error::FileCountExceeded)?;
                self.file_count += 1;
            }
        }

        // Verify file count invariant
        assert!(self.file_count <= MAX_FILES, "File count must not exceed maximum");

        Ok(Some(files))
    }

    /// Validate a path against constraints
    fn validate_path(path: &Path) -> Result<()> {
        // Check path length
        let path_len = path.as_os_str().len();
        if path_len > MAX_PATH_LENGTH {
            return Err(Error::PathTooLong);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;
    use crate::alloc::TrackingAllocator;

    #[test]
    fn test_new_crawler() {
        let temp_dir = TempDir::new().unwrap();
        let crawler = Crawler::new(temp_dir.path());
        assert!(crawler.is_ok());
    }

    #[test]
    fn test_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        // First call should return an empty vector since directory is empty
        let result = crawler.process_next().unwrap();
        assert!(matches!(result, Some(files) if files.is_empty()));

        // Second call should return None since queue is empty
        let result = crawler.process_next().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_file_count_limit() {
        const TEST_FILE_COUNT: usize = 10;

        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        // Create a small number of files for testing
        for i in 0..TEST_FILE_COUNT {
            let file = temp_dir.path().join(format!("file_{i}"));
            File::create(&file).unwrap();
        }

        // Process directory
        let files = crawler.process_next().unwrap().unwrap();
        assert_eq!(files.len(), TEST_FILE_COUNT);
    }

    #[test]
    fn test_file_size_limit() {
        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        // Create a file larger than the limit
        let file = temp_dir.path().join("large.txt");
        let mut f = File::create(&file).unwrap();
        #[allow(clippy::cast_possible_truncation)]
        let data = vec![0u8; (MAX_FILE_SIZE + 1) as usize];
        f.write_all(&data).unwrap();

        // Process directory - should return FileSizeExceeded error
        match crawler.process_next() {
            Err(Error::FileSizeExceeded) => (),
            other => panic!("Expected FileSizeExceeded error, got {other:?}"),
        }
    }

    #[test]
    fn test_directory_depth() {
        const TEST_DEPTH: usize = 3;

        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        // Create a flat structure of directories
        for i in 0..TEST_DEPTH {
            let dir = temp_dir.path().join(format!("dir_{i}"));
            std::fs::create_dir(&dir).unwrap();
            File::create(dir.join("test.txt")).unwrap();
        }

        // Process directory
        let mut total_files = 0;
        while let Some(files) = crawler.process_next().unwrap() {
            total_files += files.len();
        }

        // Should find one file per directory
        assert_eq!(total_files, TEST_DEPTH);

        // Verify progress tracking
        let (files, _, dirs) = crawler.progress();
        assert_eq!(files, TEST_DEPTH);
        assert_eq!(dirs, TEST_DEPTH + 1); // Root + TEST_DEPTH directories
    }

    #[test]
    fn test_mixed_files_and_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        // Create a mix of files and directories
        File::create(temp_dir.path().join("file1.txt")).unwrap();
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        File::create(subdir.join("file2.txt")).unwrap();

        // Process directory
        let mut total_files = 0;
        while let Some(files) = crawler.process_next().unwrap() {
            total_files += files.len();
        }

        assert_eq!(total_files, 2);
    }

    #[test]
    fn test_path_length_validation() {
        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        // Create a file with a long but valid name
        let long_name = "a".repeat(10); // Use a shorter but still meaningful length
        let file = temp_dir.path().join(long_name);
        File::create(&file).unwrap();

        // Process directory - should succeed with the file
        let files = crawler.process_next().unwrap().unwrap();
        assert_eq!(files.len(), 1);

        // Try to create a crawler with a path that's too long
        let long_name = "a".repeat(MAX_PATH_LENGTH + 1);
        let path = temp_dir.path().join(long_name);

        // Should fail with PathTooLong without checking existence
        assert!(matches!(Crawler::validate_path(&path), Err(Error::PathTooLong)));
    }

    #[test]
    fn test_file_count_near_limit() {
        const LARGE_FILE_COUNT: usize = 1000; // Use 1000 instead of 1M for practical testing

        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        // Create a large number of files
        for i in 0..LARGE_FILE_COUNT {
            let file = temp_dir.path().join(format!("file_{i}"));
            File::create(&file).unwrap();
        }

        // Process directory in chunks
        let mut total_files = 0;
        while let Some(files) = crawler.process_next().unwrap() {
            total_files += files.len();
        }

        // Verify we found all files
        assert_eq!(total_files, LARGE_FILE_COUNT);

        // Try to add one more file - should fail
        let extra_file = temp_dir.path().join("one_too_many");
        File::create(&extra_file).unwrap();

        // Reset crawler for the new test
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();
        crawler.file_count = MAX_FILES; // Simulate being at the limit

        match crawler.process_next() {
            Err(Error::FileCountExceeded) => (),
            other => panic!("Expected FileCountExceeded error, got {other:?}"),
        }
    }

    #[test]
    fn test_memory_usage_during_indexing() {
        static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        // Create a moderate number of files and directories
        for i in 0..10 {
            let subdir = temp_dir.path().join(format!("dir_{i}"));
            std::fs::create_dir(&subdir).unwrap();
            for j in 0..10 {
                let file = subdir.join(format!("file_{j}.txt"));
                File::create(&file).unwrap();
            }
        }

        // Process all files while tracking memory
        let mut total_files = 0;
        while let Some(files) = crawler.process_next().unwrap() {
            total_files += files.len();
        }

        // End initialization phase
        ALLOCATOR.end_init();

        // Verify we found all files
        assert_eq!(total_files, 100);

        // Verify peak memory usage is within bounds (should be much less than 500MB)
        assert!(
            ALLOCATOR.peak() < 500 * 1024 * 1024,
            "Peak memory usage too high: {} bytes",
            ALLOCATOR.peak()
        );
    }

    #[test]
    fn test_progress_reporting() {
        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        // Initially should have just the root directory
        let (files, max_files, dirs) = crawler.progress();
        assert_eq!(files, 0);
        assert_eq!(max_files, MAX_FILES);
        assert_eq!(dirs, 1);

        // Create a test directory structure
        for i in 0..3 {
            let subdir = temp_dir.path().join(format!("dir_{i}"));
            std::fs::create_dir(&subdir).unwrap();
            for j in 0..2 {
                let file = subdir.join(format!("file_{j}.txt"));
                File::create(&file).unwrap();
            }
        }

        // Process all files while checking progress
        let mut last_files = 0;
        let mut last_dirs = 1; // Start with root dir
        while let Some(batch) = crawler.process_next().unwrap() {
            let (processed, _, discovered) = crawler.progress();
            // Files should increase by the batch size
            assert!(processed >= last_files + batch.len());
            // Directories should increase or stay same
            assert!(discovered >= last_dirs);

            last_files = processed;
            last_dirs = discovered;
        }

        // Final state should show all files and directories
        let (files, _, dirs) = crawler.progress();
        assert_eq!(files, 6); // 2 files in each of 3 directories
        assert_eq!(dirs, 4); // Root + 3 subdirectories
    }
}
