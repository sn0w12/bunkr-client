use std::{collections::HashMap, time::Instant, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Table, Row, TableState},
    Terminal,
};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
};
use std::io;
use crate::core::types::FailedUploadInfo;
use webbrowser;

#[derive(Clone)]
pub enum UploadStatus {
    Ongoing(f64),
    Completed,
    Failed(FailedUploadInfo),
}

pub struct UIState {
    pub total_files: usize,
    pub uploaded_files: usize,
    pub uploaded_bytes: u64,
    pub total_bytes: u64,
    pub start_time: Instant,
    pub all_uploads: HashMap<String, UploadStatus>,
    pub album_id: Option<String>,
    pub file_sizes: HashMap<String, u64>,
    pub completed_urls: HashMap<String, String>,
}

impl UIState {
    pub fn new(total_files: usize, album_id: Option<String>, total_bytes: u64) -> Self {
        Self {
            total_files,
            uploaded_files: 0,
            uploaded_bytes: 0,
            total_bytes,
            start_time: Instant::now(),
            all_uploads: HashMap::new(),
            album_id,
            file_sizes: HashMap::new(),
            completed_urls: HashMap::new(),
        }
    }

    pub fn add_current(&mut self, name: String, progress: f64, size: u64) {
        self.all_uploads.insert(name.clone(), UploadStatus::Ongoing(progress));
        self.file_sizes.insert(name, size);
    }

    pub fn update_progress(&mut self, name: &str, progress: f64) {
        if let Some(UploadStatus::Ongoing(ref mut p)) = self.all_uploads.get_mut(name) {
            *p = progress;
        }
    }

    pub fn remove_current(&mut self, name: &str, url: Option<&str>) {
        self.all_uploads.insert(name.to_string(), UploadStatus::Completed);
        self.uploaded_files += 1;
        if let Some(url) = url {
            self.completed_urls.insert(name.to_string(), url.to_string());
        }
    }

    pub fn add_uploaded_bytes(&mut self, bytes: u64) {
        self.uploaded_bytes += bytes;
    }

    pub fn add_failed(&mut self, name: String, info: FailedUploadInfo) {
        self.all_uploads.insert(name, UploadStatus::Failed(info));
    }
}

fn format_size(size: u64) -> String {
    if size >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if size >= 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{} B", size)
    }
}

pub struct UI {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    table_state: TableState,
    previous_row_count: usize,
}

impl UI {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal, table_state: TableState::default(), previous_row_count: 0 })
    }

    pub fn draw(&mut self, state: &UIState) -> Result<(), Box<dyn std::error::Error>> {
        self.terminal.draw(|f| {
            let size = f.area();
            let speed = if state.start_time.elapsed().as_secs_f64() > 0.0 { state.uploaded_bytes as f64 / state.start_time.elapsed().as_secs_f64() / 1_000_000.0 } else { 0.0 };

            let remaining_bytes: u64 = state.total_bytes.saturating_sub(state.uploaded_bytes);

            let eta_str = if remaining_bytes > 0 && speed > 0.0 {
                let time_left_seconds = remaining_bytes as f64 / (speed * 1_000_000.0);
                if time_left_seconds < 60.0 {
                    format!(" | ETA: {:.0}s", time_left_seconds)
                } else if time_left_seconds < 3600.0 {
                    format!(" | ETA: {:.0}m", time_left_seconds / 60.0)
                } else {
                    format!(" | ETA: {:.1}h", time_left_seconds / 3600.0)
                }
            } else {
                String::new()
            };

            let header_text = if let Some(album) = &state.album_id {
                format!("Bunkr Uploader | Album: {} | Uploaded: {}/{} | Speed: {:.2} MB/s{}", album, state.uploaded_files, state.total_files, speed, eta_str)
            } else {
                format!("Bunkr Uploader | Uploaded: {}/{} | Speed: {:.2} MB/s{}", state.uploaded_files, state.total_files, speed, eta_str)
            };
            let header = Paragraph::new(header_text)
                .block(Block::default().borders(Borders::ALL).title("Header"))
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

            let header_height = 3;
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(header_height), Constraint::Min(0)])
                .split(size);

            f.render_widget(header, chunks[0]);

            let list_area = chunks[1];

            let mut all_items_vec: Vec<(&String, &UploadStatus)> = state.all_uploads.iter().collect();
            all_items_vec.sort_by(|a, b| a.0.cmp(b.0));

            let current_row_count = all_items_vec.len();

            let rows: Vec<Row> = all_items_vec.iter().map(|(name, status)| {
                let file_name = std::path::Path::new(name).file_name().unwrap_or(std::ffi::OsStr::new(name)).to_string_lossy();
                let size = match status {
                    UploadStatus::Failed(info) => info.file_size,
                    _ => *state.file_sizes.get(*name).unwrap_or(&0),
                };
                let size_str = format_size(size);
                let (progress_str, status_str, url_str) = match status {
                    UploadStatus::Ongoing(progress) => (format!("{:.0}%", progress * 100.0), "Ongoing".to_string(), "".to_string()),
                    UploadStatus::Completed => {
                        let url = state.completed_urls.get(*name).cloned().unwrap_or_else(|| "".to_string());
                        ("100%".to_string(), "Completed".to_string(), url)
                    }
                    UploadStatus::Failed(info) => {
                        let status_str_inner = if let Some(code) = info.status_code {
                            format!(" (HTTP {})", code)
                        } else {
                            String::new()
                        };
                        ("".to_string(), format!("Failed{}: {}", status_str_inner, info.error), "".to_string())
                    }
                };
                Row::new(vec![file_name.to_string(), size_str, progress_str, status_str, url_str])
            }).collect();

            let widths = [
                Constraint::Percentage(25),
                Constraint::Percentage(12),
                Constraint::Percentage(12),
                Constraint::Percentage(25),
                Constraint::Percentage(26),
            ];

            let table = Table::new(rows, widths)
                .block(Block::default().borders(Borders::ALL).title("Uploads"))
                .header(
                    Row::new(vec!["File", "Size", "Progress", "Status", "URL"])
                        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                )
                .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

            if let Some(selected) = self.table_state.selected() {
                if selected == self.previous_row_count.saturating_sub(1) && current_row_count > self.previous_row_count {
                    self.table_state.select(Some(current_row_count - 1));
                }
            }

            self.previous_row_count = current_row_count;

            f.render_stateful_widget(table, list_area, &mut self.table_state);
        })?;
        Ok(())
    }

    pub fn restore(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}

pub fn start_ui(ui_state: Arc<Mutex<UIState>>) -> (std::thread::JoinHandle<()>, Arc<AtomicBool>) {
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    let ui_state_clone = ui_state.clone();
    let handle = std::thread::spawn(move || {
        let mut ui = UI::new().unwrap();
        while running_clone.load(Ordering::Relaxed) {
            if event::poll(std::time::Duration::from_millis(0)).unwrap_or(false) {
                if let Ok(Event::Key(key_event)) = event::read() {
                    if key_event.kind == KeyEventKind::Press || key_event.kind == KeyEventKind::Repeat {
                        match key_event.code {
                            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                                running_clone.store(false, Ordering::Relaxed);
                                break;
                            }
                            KeyCode::Up => {
                                let selected = ui.table_state.selected().unwrap_or(0);
                                if selected > 0 {
                                    ui.table_state.select(Some(selected - 1));
                                }
                            }
                            KeyCode::Down => {
                                let selected = ui.table_state.selected().unwrap_or(0);
                                ui.table_state.select(Some(selected + 1));
                            }
                            KeyCode::Enter => {
                                let state = ui_state_clone.lock().unwrap();
                                let mut all_items_vec: Vec<(&String, &UploadStatus)> = state.all_uploads.iter().collect();
                                all_items_vec.sort_by(|a, b| a.0.cmp(b.0));
                                if let Some(selected) = ui.table_state.selected() {
                                    if selected < all_items_vec.len() {
                                        let (name, status) = &all_items_vec[selected];
                                        if let UploadStatus::Completed = status {
                                            if let Some(url) = state.completed_urls.get(*name) {
                                                let _ = webbrowser::open(url);
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            {
                let state = ui_state_clone.lock().unwrap();
                ui.draw(&state).unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        ui.restore().unwrap();
    });
    (handle, running)
}