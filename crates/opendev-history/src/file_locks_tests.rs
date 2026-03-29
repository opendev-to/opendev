use super::*;
use tempfile::NamedTempFile;

#[test]
fn test_acquire_and_release() {
    let tmp = NamedTempFile::new().unwrap();
    let lock = FileLock::acquire(tmp.path(), Duration::from_secs(5)).unwrap();
    lock.release();
}

#[test]
fn test_with_file_lock() {
    let tmp = NamedTempFile::new().unwrap();
    let result = with_file_lock(tmp.path(), Duration::from_secs(5), || 42).unwrap();
    assert_eq!(result, 42);
}

#[test]
fn test_lock_dropped_on_scope_exit() {
    let tmp = NamedTempFile::new().unwrap();
    {
        let _lock = FileLock::acquire(tmp.path(), Duration::from_secs(5)).unwrap();
    }
    // Lock should be released after scope exit
    let _lock2 = FileLock::acquire(tmp.path(), Duration::from_secs(1)).unwrap();
}
