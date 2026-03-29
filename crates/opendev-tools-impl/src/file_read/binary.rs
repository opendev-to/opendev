//! Binary file detection by extension and content inspection.

/// Known binary file extensions (fast path to avoid reading content).
pub(super) const BINARY_EXTENSIONS: &[&str] = &[
    // Archives & compressed
    "zip", "gz", "tar", "bz2", "xz", "7z", "rar", "zst", "lz4", // Images
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "tiff", "tif", "avif", "heic",
    // Audio/Video
    "mp3", "mp4", "wav", "ogg", "flac", "avi", "mkv", "mov", "webm",
    // Executables & libraries
    "exe", "dll", "so", "dylib", "o", "a", "lib", "class", // Compiled/bytecode
    "pyc", "pyo", "wasm", "beam", // Databases
    "db", "sqlite", "sqlite3", // Documents (binary)
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", // Fonts
    "ttf", "otf", "woff", "woff2", "eot", // Other binary
    "bin", "dat", "pak", "jar", "war", "egg", // Serialized data
    "pb", "protobuf", "flatbuf", "msgpack", // Lock files (often large, not useful)
    "lock",
];

/// Check if a file is likely binary, first by extension, then by content inspection.
pub(super) fn is_binary_file(path: &std::path::Path, bytes: &[u8]) -> bool {
    // Fast path: check extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_ascii_lowercase();
        if BINARY_EXTENSIONS.contains(&ext_lower.as_str()) {
            return true;
        }
    }
    // Content-based: check for null bytes in first 8 KB
    is_binary(bytes)
}

/// Check if content appears to be binary.
///
/// Uses two heuristics (matching OpenCode's approach):
/// 1. Null bytes in the first 8KB -> definitely binary.
/// 2. More than 30% non-printable characters -> likely binary.
///
/// Non-printable is defined as bytes < 9 or (> 13 and < 32), excluding
/// tab (9), newline (10), carriage return (13).
pub(super) fn is_binary(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(8192);
    if check_len == 0 {
        return false;
    }
    let sample = &bytes[..check_len];

    // Check for null bytes (fast path)
    if sample.contains(&0) {
        return true;
    }

    // Check non-printable character ratio
    let non_printable = sample
        .iter()
        .filter(|&&b| b < 9 || (b > 13 && b < 32))
        .count();

    let ratio = non_printable as f64 / check_len as f64;
    ratio > 0.3
}

#[cfg(test)]
#[path = "binary_tests.rs"]
mod tests;
