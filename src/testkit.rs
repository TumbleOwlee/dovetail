//! Test-only helpers: a unique scratch directory per test, removed on drop.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("prodgy-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir is creatable");
        TempDir(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn write(&self, rel: &str, content: &str) -> PathBuf {
        let p = self.0.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("parent is creatable");
        }
        std::fs::write(&p, content).expect("file is writable");
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// Renders one frame at `width`x`height` and returns the drawn buffer.
pub fn render_buffer<F: FnOnce(&mut ratatui::Frame)>(
    width: u16,
    height: u16,
    render: F,
) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
    terminal.draw(render).expect("draw");
    terminal.backend().buffer().clone()
}

/// Renders one frame at `width`x`height` and returns the rows as trimmed-right strings.
pub fn render_rows<F: FnOnce(&mut ratatui::Frame)>(
    width: u16,
    height: u16,
    render: F,
) -> Vec<String> {
    buffer_rows(&render_buffer(width, height, render))
}

/// The buffer's rows as trimmed-right strings.
pub fn buffer_rows(buf: &ratatui::buffer::Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}
