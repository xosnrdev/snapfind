use std::fs;
use std::path::{Path, PathBuf};

use arrayvec::ArrayVec;

use super::error::{Error, Result};

pub const MAX_DEPTH: usize = 1_000;
pub const MAX_FILES: usize = 1_000;
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
pub const MAX_PATH_LENGTH: usize = 255;

#[derive(Debug)]
pub struct Crawler {
    queue: ArrayVec<(PathBuf, usize), MAX_DEPTH>,
    file_count: usize,
    dir_count: usize,
}

impl Crawler {
    pub fn new(start_path: &Path) -> Result<Self> {
        Self::validate_path(start_path)?;

        let mut queue = ArrayVec::new();
        queue
            .try_push((start_path.to_path_buf(), 0))
            .map_err(|_| Error::DepthExceeded)?;

        Ok(Self {
            queue,
            file_count: 0,
            dir_count: 1,
        })
    }

    #[must_use = "Progress information should be used for monitoring"]
    pub const fn progress(&self) -> (usize, usize, usize) {
        (self.file_count, MAX_FILES, self.dir_count)
    }

    pub fn process_next(&mut self) -> Result<Option<ArrayVec<PathBuf, MAX_FILES>>> {
        let Some((dir, current_depth)) = self.queue.pop() else {
            return Ok(None);
        };

        assert!(dir.exists(), "Directory in queue must exist");
        assert!(dir.is_dir(), "Path in queue must be a directory");

        let mut files = ArrayVec::new();

        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();

            Self::validate_path(&path)?;

            if entry.file_type()?.is_dir() {
                let new_depth = current_depth + 1;
                if new_depth >= MAX_DEPTH {
                    return Err(Error::DepthExceeded);
                }
                self.queue
                    .try_push((path, new_depth))
                    .map_err(|_| Error::DepthExceeded)?;
                self.dir_count += 1;
            } else {
                if self.file_count >= MAX_FILES {
                    return Err(Error::FileCountExceeded);
                }
                let size = entry.metadata()?.len();
                if size > MAX_FILE_SIZE {
                    return Err(Error::FileSizeExceeded);
                }
                files.try_push(path).map_err(|_| Error::FileCountExceeded)?;
                self.file_count += 1;
            }
        }

        assert!(
            self.file_count <= MAX_FILES,
            "File count must not exceed maximum"
        );

        Ok(Some(files))
    }

    fn validate_path(path: &Path) -> Result<()> {
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

        let result = crawler.process_next().unwrap();
        assert!(matches!(result, Some(files) if files.is_empty()));

        let result = crawler.process_next().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_file_count_limit() {
        const TEST_FILE_COUNT: usize = 10;

        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        for i in 0..TEST_FILE_COUNT {
            let file = temp_dir.path().join(format!("file_{i}"));
            File::create(&file).unwrap();
        }

        let files = crawler.process_next().unwrap().unwrap();
        assert_eq!(files.len(), TEST_FILE_COUNT);
    }

    #[test]
    fn test_file_size_limit() {
        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        let file = temp_dir.path().join("large.txt");
        let mut f = File::create(&file).unwrap();
        #[allow(clippy::cast_possible_truncation)]
        let data = vec![0u8; (MAX_FILE_SIZE + 1) as usize];
        f.write_all(&data).unwrap();

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

        for i in 0..TEST_DEPTH {
            let dir = temp_dir.path().join(format!("dir_{i}"));
            fs::create_dir(&dir).unwrap();
            File::create(dir.join("test.txt")).unwrap();
        }

        let mut total_files = 0;
        while let Some(files) = crawler.process_next().unwrap() {
            total_files += files.len();
        }

        assert_eq!(total_files, TEST_DEPTH);

        let (files, _, dirs) = crawler.progress();
        assert_eq!(files, TEST_DEPTH);
        assert_eq!(dirs, TEST_DEPTH + 1);
    }

    #[test]
    fn test_mixed_files_and_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        File::create(temp_dir.path().join("file1.txt")).unwrap();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        File::create(subdir.join("file2.txt")).unwrap();

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

        let long_name = "a".repeat(10);
        let file = temp_dir.path().join(long_name);
        File::create(&file).unwrap();

        let files = crawler.process_next().unwrap().unwrap();
        assert_eq!(files.len(), 1);

        let long_name = "a".repeat(MAX_PATH_LENGTH + 1);
        let path = temp_dir.path().join(long_name);

        assert!(matches!(
            Crawler::validate_path(&path),
            Err(Error::PathTooLong)
        ));
    }

    #[test]
    fn test_file_count_near_limit() {
        const LARGE_FILE_COUNT: usize = 1000;

        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        for i in 0..LARGE_FILE_COUNT {
            let file = temp_dir.path().join(format!("file_{i}"));
            File::create(&file).unwrap();
        }

        let mut total_files = 0;
        while let Some(files) = crawler.process_next().unwrap() {
            total_files += files.len();
        }

        assert_eq!(total_files, LARGE_FILE_COUNT);

        let extra_file = temp_dir.path().join("one_too_many");
        File::create(&extra_file).unwrap();

        let mut crawler = Crawler::new(temp_dir.path()).unwrap();
        crawler.file_count = MAX_FILES;

        match crawler.process_next() {
            Err(Error::FileCountExceeded) => (),
            other => panic!("Expected FileCountExceeded error, got {other:?}"),
        }
    }

    #[test]
    fn test_progress_reporting() {
        let temp_dir = TempDir::new().unwrap();
        let mut crawler = Crawler::new(temp_dir.path()).unwrap();

        let (files, max_files, dirs) = crawler.progress();
        assert_eq!(files, 0);
        assert_eq!(max_files, MAX_FILES);
        assert_eq!(dirs, 1);

        for i in 0..3 {
            let subdir = temp_dir.path().join(format!("dir_{i}"));
            fs::create_dir(&subdir).unwrap();
            for j in 0..2 {
                let file = subdir.join(format!("file_{j}.txt"));
                File::create(&file).unwrap();
            }
        }

        let mut last_files = 0;
        let mut last_dirs = 1;
        while let Some(batch) = crawler.process_next().unwrap() {
            let (processed, _, discovered) = crawler.progress();
            assert!(processed >= last_files + batch.len());
            assert!(discovered >= last_dirs);

            last_files = processed;
            last_dirs = discovered;
        }

        let (files, _, dirs) = crawler.progress();
        assert_eq!(files, 6);
        assert_eq!(dirs, 4);
    }
}
