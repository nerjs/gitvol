use std::io::{Result, Stderr, StderrLock, Stdout, StdoutLock, Write, stderr, stdout};
use tracing::{Level, Metadata, level_filters::LevelFilter};
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::{MakeWriter, layer},
    layer::SubscriberExt,
    registry,
    util::SubscriberInitExt,
};

enum StdioLock<'a> {
    Stdout(StdoutLock<'a>),
    Stderr(StderrLock<'a>),
}

impl<'a> Write for StdioLock<'a> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        match self {
            StdioLock::Stdout(lock) => lock.write(buf),
            StdioLock::Stderr(lock) => lock.write(buf),
        }
    }

    fn flush(&mut self) -> Result<()> {
        match self {
            StdioLock::Stdout(lock) => lock.flush(),
            StdioLock::Stderr(lock) => lock.flush(),
        }
    }
}

struct SplitMakeWriter {
    stdout: Stdout,
    stderr: Stderr,
}

impl SplitMakeWriter {
    fn new() -> Self {
        Self {
            stdout: stdout(),
            stderr: stderr(),
        }
    }
}

impl<'a> MakeWriter<'a> for SplitMakeWriter {
    type Writer = StdioLock<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        StdioLock::Stdout(self.stdout.lock())
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        let level = *meta.level();

        if level == Level::ERROR || level == Level::WARN {
            StdioLock::Stderr(self.stderr.lock())
        } else {
            StdioLock::Stdout(self.stdout.lock())
        }
    }
}

pub fn init() {
    registry()
        .with(
            layer()
                .without_time()
                .with_ansi(cfg!(debug_assertions))
                .with_writer(SplitMakeWriter::new())
                .with_filter(
                    EnvFilter::builder()
                        .with_default_directive(LevelFilter::DEBUG.into())
                        .from_env_lossy(),
                ),
        )
        .init()
}
