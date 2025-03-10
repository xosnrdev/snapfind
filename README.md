# SnapFind

Fast file search tool that understands content.

## Features

- Content-aware search (understands text, markdown, source code, config files)
- Low memory usage (<500KB)
- Fast indexing and search
- No external dependencies

## Install

```bash
cargo install snapfind
```

## Usage

Index a directory:

```bash
snapfind index /path/to/dir
```

Search for files:

```bash
snapfind search "your query" /path/to/dir
```

## Limits

- Maximum file size: 10MB
- Supported content: Plain text, Markdown, Source code, Config files
- Binary files are automatically skipped

## Examples

Search by content:

```bash
# Find code
snapfind search "fn main" ~/code

# Find documentation
snapfind search "# Introduction" ~/docs
```

## Common Messages

Errors:

- "Directory not found": Check if the path exists
- "No index found": Run `snapfind index` first
- "File too large": Files over 10MB are skipped

Tips:

- Index before searching
- Use quotes for multi-word queries
- Check file permissions if indexing fails

## License

[MIT](./LICENSE)
