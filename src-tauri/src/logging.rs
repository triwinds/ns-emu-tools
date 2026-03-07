//! 日志管理模块
//!
//! 提供统一的日志配置和初始化功能

use chrono::Local;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    fmt::{self, format::Writer as FormatWriter, time::FormatTime},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

const MAX_LOG_FILE_BYTES: u64 = 5 * 1_000_000;

#[derive(Clone, Copy, Debug, Default)]
struct LocalTimeFormatter;

impl FormatTime for LocalTimeFormatter {
    fn format_time(&self, writer: &mut FormatWriter<'_>) -> std::fmt::Result {
        write!(
            writer,
            "{}",
            Local::now().format("%Y-%m-%d %H:%M:%S%.3f %:z")
        )
    }
}

#[derive(Debug)]
struct CappedFileAppender {
    state: Mutex<LogFileState>,
}

#[derive(Debug)]
struct LogFileState {
    path: PathBuf,
    max_bytes: u64,
    current_size: u64,
    file: Option<File>,
}

impl CappedFileAppender {
    fn new(path: PathBuf, max_bytes: u64) -> io::Result<Self> {
        Ok(Self {
            state: Mutex::new(LogFileState::new(path, max_bytes)?),
        })
    }
}

impl Write for CappedFileAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("日志文件写入锁已损坏"))?;
        state.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("日志文件写入锁已损坏"))?;
        state.flush()
    }
}

impl LogFileState {
    fn new(path: PathBuf, max_bytes: u64) -> io::Result<Self> {
        let current_size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);

        let mut state = Self {
            path,
            max_bytes,
            current_size,
            file: None,
        };

        if current_size >= max_bytes {
            state.rotate()?;
        } else {
            state.file = Some(open_append_file(&state.path)?);
        }

        Ok(state)
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.current_size > 0 && self.current_size + buf.len() as u64 > self.max_bytes {
            self.rotate()?;
        }

        let file = self.ensure_file()?;
        let written = file.write(buf)?;
        self.current_size += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()?;
        }
        Ok(())
    }

    fn ensure_file(&mut self) -> io::Result<&mut File> {
        if self.file.is_none() {
            self.file = Some(open_append_file(&self.path)?);
        }
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("日志文件尚未初始化"))
    }

    fn rotate(&mut self) -> io::Result<()> {
        if let Some(mut file) = self.file.take() {
            file.flush()?;
        }

        rotate_log_file(&self.path, self.current_size)?;

        self.file = Some(open_truncated_file(&self.path)?);
        self.current_size = 0;
        Ok(())
    }
}

fn rotate_log_file(path: &Path, current_size: u64) -> io::Result<()> {
    if current_size == 0 || !path.exists() {
        return Ok(());
    }

    let backup_path = backup_log_path(path)?;

    if backup_path.exists() {
        fs::remove_file(&backup_path)?;
    }

    fs::rename(path, backup_path)
}

fn backup_log_path(path: &Path) -> io::Result<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::other("日志文件路径缺少文件名"))?;

    Ok(path.with_file_name(format!("{}.1", file_name.to_string_lossy())))
}

fn open_append_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn open_truncated_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
}

/// 获取日志文件路径
fn log_file_path() -> PathBuf {
    // 跟随 config.json 使用相同目录
    let dir = crate::config::effective_config_dir();

    // 确保目录存在
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("创建日志目录失败: {}", e);
    }

    dir.join("ns-emu-tools.log")
}

/// 初始化日志系统
///
/// 配置项：
/// - 默认级别：info
/// - ns_emu_tools 模块：debug
/// - aria2_ws::client：info（避免过多调试日志）
///
/// 日志输出：
/// - 同时输出到控制台和文件 `ns-emu-tools.log`
/// - 日志时间使用系统当前时区
/// - 日志文件按大小轮转，当前文件最大 5 MB，并保留最近 1 个备份文件 `ns-emu-tools.log.1`
///
/// 可以通过环境变量 `RUST_LOG` 覆盖默认配置
///
/// 返回 WorkerGuard，用于保持日志写入器活跃
pub fn init() -> WorkerGuard {
    // 设置日志过滤器
    // 如果有 RUST_LOG 环境变量，使用它；否则使用默认配置
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // 默认配置：
        // - 全局默认 info 级别
        // - ns_emu_tools 模块设置为 debug
        // - aria2_ws::client 设置为 info（避免 debug/trace 日志过多）
        EnvFilter::new("info,ns_emu_tools=debug,aria2_ws::client=info")
    });

    // 创建文件 appender
    let log_path = log_file_path();

    let file_appender = CappedFileAppender::new(log_path.clone(), MAX_LOG_FILE_BYTES)
        .unwrap_or_else(|e| panic!("初始化日志文件失败（{}）: {}", log_path.display(), e));
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // 配置文件日志格式
    let file_layer = fmt::layer()
        .with_timer(LocalTimeFormatter)
        .with_writer(non_blocking)
        .with_target(true) // 显示日志来源模块
        .with_thread_ids(false) // 不显示线程 ID
        .with_file(true) // 显示文件名
        .with_line_number(true) // 显示行号
        .with_ansi(false); // 文件中不使用 ANSI 颜色

    // 配置控制台日志格式
    let console_layer = fmt::layer()
        .with_timer(LocalTimeFormatter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(true); // 控制台使用 ANSI 颜色

    // 初始化 tracing subscriber，同时输出到控制台和文件
    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    guard
}

/// 测试环境下的日志初始化
///
/// 仅用于测试，使用更详细的日志级别
#[cfg(test)]
pub fn init_test() {
    use once_cell::sync::OnceCell;

    static TRACING_INIT: OnceCell<()> = OnceCell::new();

    TRACING_INIT.get_or_init(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("debug,aria2_ws::client=info"));

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_test_writer())
            .init();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tracing::{debug, info};

    #[test]
    fn test_logging_initialization() {
        init_test();
        info!("测试日志初始化");
        debug!("调试级别日志");
    }

    #[test]
    fn test_rotate_when_file_exceeds_limit() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("ns-emu-tools.log");
        let mut writer = CappedFileAppender::new(log_path.clone(), 32).unwrap();

        writer.write_all(b"1234567890\n").unwrap();
        writer.write_all(b"abcdefghijklmnopqrstuvwxyz\n").unwrap();
        writer.flush().unwrap();

        let current = fs::read_to_string(&log_path).unwrap();
        let backup = fs::read_to_string(log_path.with_file_name("ns-emu-tools.log.1")).unwrap();

        assert_eq!(current, "abcdefghijklmnopqrstuvwxyz\n");
        assert_eq!(backup, "1234567890\n");
        assert!(fs::metadata(&log_path).unwrap().len() <= 32);
    }

    #[test]
    fn test_rotate_oversized_file_on_startup() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("ns-emu-tools.log");
        fs::write(&log_path, vec![b'x'; 40]).unwrap();

        let mut writer = CappedFileAppender::new(log_path.clone(), 32).unwrap();
        writer.write_all(b"ok\n").unwrap();
        writer.flush().unwrap();

        assert_eq!(fs::read(&log_path).unwrap(), b"ok\n");
        assert_eq!(fs::read(&log_path.with_file_name("ns-emu-tools.log.1")).unwrap(), vec![b'x'; 40]);
    }

    #[test]
    fn test_only_keep_latest_backup() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("ns-emu-tools.log");
        let mut writer = CappedFileAppender::new(log_path.clone(), 10).unwrap();

        writer.write_all(b"11111\n").unwrap();
        writer.write_all(b"22222\n").unwrap();
        writer.write_all(b"33333\n").unwrap();
        writer.flush().unwrap();

        assert_eq!(fs::read_to_string(&log_path).unwrap(), "33333\n");
        assert_eq!(fs::read_to_string(log_path.with_file_name("ns-emu-tools.log.1")).unwrap(), "22222\n");
    }
}
