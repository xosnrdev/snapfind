# SnapFind

Fast, memory-efficient file search utility with predictable resource usage.

## Features

- [x] Content-aware search with relevance scoring
- [x] Fixed memory bounds (no dynamic allocations after initialization)
- [x] Text type detection (plain text, markdown, source code, config files)
- [x] Glob pattern matching
- [x] Fast indexing with bounded resource usage

## Install

```bash
cargo install snapfind
```

## Usage

Index a directory:

```bash
snapfind index [DIR]
```

Search for files:

```bash
snapfind search "your query" [DIR]
```

## Limitations

- Maximum number of files: 1,000
- Maximum directory depth: 1,000
- Maximum file size: 10MB
- Maximum indexed content: 1,000 bytes per file
- Maximum query length: 50 bytes
- Only handles text files (binary files are excluded)

## Examples

Search by content:

```bash
# Find code
snapfind search "fn main" ~/code

# Find documentation
snapfind search "# Introduction" ~/docs

# Use glob patterns
snapfind search "*.txt" ~/documents
```

## License

[MIT License](./LICENSE)
