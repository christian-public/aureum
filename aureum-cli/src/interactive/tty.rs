use crate::interactive::utils::frame;
use crossterm::event::{Event, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::layout::Rect;
use std::io::{self, BufRead, Stdout, Write};

/// Abstracts the differences between an interactive terminal session and a `--record`
/// session driven from stdin/stdout. View functions take `&mut dyn Tty` and don't care
/// which backs them: rendering goes to the right place, and key events arrive uniformly.
pub(crate) trait Tty {
    fn area(&self) -> io::Result<Rect>;
    fn draw(&mut self, draw_fn: &mut dyn FnMut(&mut Frame)) -> io::Result<()>;
    /// Blocks until the next key. Returns `None` on EOF (record mode only).
    fn next_key(&mut self) -> io::Result<Option<KeyEvent>>;
}

/// Live interactive session backed by a crossterm terminal.
pub(crate) struct LiveTty<'a> {
    pub(crate) terminal: &'a mut Terminal<CrosstermBackend<Stdout>>,
}

impl Tty for LiveTty<'_> {
    fn area(&self) -> io::Result<Rect> {
        let size = self.terminal.size()?;
        Ok(Rect::new(0, 0, size.width, size.height))
    }

    fn draw(&mut self, draw_fn: &mut dyn FnMut(&mut Frame)) -> io::Result<()> {
        self.terminal.draw(draw_fn)?;
        Ok(())
    }

    fn next_key(&mut self) -> io::Result<Option<KeyEvent>> {
        loop {
            if let Event::Key(key) = crossterm::event::read()?
                && key.kind == KeyEventKind::Press
            {
                return Ok(Some(key));
            }
        }
    }
}

/// Headless session for `--record`. Renders into a `TestBackend`, writes each frame to
/// `writer` separated by `---`, and reads one key-name per line from `reader`.
pub(crate) struct RecordTty<'a, R: BufRead, W: Write> {
    terminal: Terminal<TestBackend>,
    reader: &'a mut R,
    writer: &'a mut W,
    width: u16,
    height: u16,
    pending_separator: Option<bool>,
}

impl<'a, R: BufRead, W: Write> RecordTty<'a, R, W> {
    pub(crate) fn new(
        width: u16,
        height: u16,
        reader: &'a mut R,
        writer: &'a mut W,
        separator_before_first_frame: bool,
    ) -> io::Result<Self> {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).map_err(io::Error::other)?;
        Ok(RecordTty {
            terminal,
            reader,
            writer,
            width,
            height,
            pending_separator: Some(separator_before_first_frame),
        })
    }

    /// First call returns the value passed to the constructor; every subsequent call
    /// returns `true` (frames after the first always need a `---` separator).
    fn take_separator(&mut self) -> bool {
        self.pending_separator.take().unwrap_or(true)
    }
}

impl<R: BufRead, W: Write> Tty for RecordTty<'_, R, W> {
    fn area(&self) -> io::Result<Rect> {
        Ok(Rect::new(0, 0, self.width, self.height))
    }

    fn draw(&mut self, draw_fn: &mut dyn FnMut(&mut Frame)) -> io::Result<()> {
        self.terminal.draw(draw_fn).map_err(io::Error::other)?;
        let sep = self.take_separator();
        frame::write_frame(
            self.terminal.backend(),
            self.width,
            self.height,
            self.writer,
            sep,
        )
    }

    fn next_key(&mut self) -> io::Result<Option<KeyEvent>> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            let key_name = line.trim();
            if key_name.is_empty() {
                continue;
            }
            if let Some(code) = frame::parse_key_name(key_name) {
                return Ok(Some(KeyEvent::new(code, KeyModifiers::NONE)));
            }
        }
    }
}
