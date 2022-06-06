extern crate anyhow;
extern crate clap;
extern crate cpal;
extern crate hound;

use cpal::Device;
use crossterm::event::KeyEvent;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use regex::Regex;
use tui::layout::Rect;
use tui::widgets::{Block, Borders};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::Paragraph,
    Frame, Terminal,
};

use anyhow::Result;
use cpal::traits::{DeviceTrait, StreamTrait};
use std::cmp::max;
use std::fs::File;
use std::io::BufWriter;
use std::sync::{Arc, Mutex};

pub fn run(device: &Device) -> Result<()> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut messages: Vec<String> = vec![];
    loop {
        let filename = &determine_filename();
        messages.push(format!("Started new recording {}", filename));
        let action = main(&mut terminal, device, filename, &mut messages)?;
        match action {
            Action::StopAndQuit => break,
            Action::CancelAndQuit => {
                messages.push(format!("Deleting {}", filename));
                std::fs::remove_file(filename).expect("Failed to remove recording");
                break;
            }

            Action::RestartRecording => {
                messages.push(format!("Deleting {}", filename));
                std::fs::remove_file(filename).expect("Failed to remove recording");
            }
            Action::StartAnotherRecording => {}
        }
    }

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(())
}

lazy_static! {
    static ref RE: Regex = Regex::new("recording-(\\d+).wav").unwrap();
}

fn determine_filename() -> String {
    let paths = std::fs::read_dir("./").unwrap();

    let mut max_number: i32 = 0;

    for path in paths {
        let filename_os = path.unwrap().file_name();
        let filename = filename_os.to_str().unwrap();
        if let Some(capture) = RE.captures(filename) {
            let number = capture[1].parse::<i32>().unwrap();
            max_number = max(max_number, number);
        }
    }

    format!("{}{}{}", "recording-", (max_number + 1).to_string(), ".wav")
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    filename: &str,
    device_name: String,
    messages: &Vec<String>,
) -> Result<Action> {
    loop {
        terminal.draw(|f| ui(f, filename, &device_name, messages))?;

        if let Event::Key(key) = event::read()? {
            match get_action(key) {
                Some(action) => return Ok(action),
                None => {}
            };
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, filename: &str, device_name: &str, messages: &Vec<String>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Length(4), Constraint::Length(3), Constraint::Min(3)].as_ref())
        .split(f.size());

    render_help(f, chunks[0]);
    render_status(f, chunks[1], filename, device_name);
    render_messages(f, chunks[2], messages);
}

fn render_status<B: Backend>(f: &mut Frame<B>, rect: Rect, filename: &str, device_name: &str) {
    let lines = vec![Spans::from(vec![
        Span::raw("Recording from device "),
        Span::styled(device_name, Style::default().fg(Color::Red)),
        Span::raw(" to file "),
        Span::styled(filename, Style::default().fg(Color::Red)),
    ])];
    f.render_widget(
        Paragraph::new(Text::from(lines)).block(Block::default().title("STATUS").borders(Borders::ALL)),
        rect,
    );
}

fn render_help<B: Backend>(f: &mut Frame<B>, rect: Rect) {
    let lines = vec![
        Spans::from(vec![
            Span::styled("Space", Style::default().fg(Color::Red)),
            Span::raw(" to start a new recording"),
        ]),
        Spans::from(vec![
            Span::styled("Backspace", Style::default().fg(Color::Red)),
            Span::raw(" to start a new recording and delete the current recording\n"),
        ]),
        Spans::from(vec![
            Span::styled("q", Style::default().fg(Color::Red)),
            Span::raw(" to stop the recording and quit the application\n"),
        ]),
        Spans::from(vec![
            Span::styled("Q", Style::default().fg(Color::Red)),
            Span::raw(" to delete the current recording and quit the application\n"),
        ]),
    ];
    f.render_widget(Paragraph::new(Text::from(lines)), rect);
}

fn render_messages<B: Backend>(f: &mut Frame<B>, rect: Rect, messages: &Vec<String>) {
    let lines: Vec<Spans> = messages.iter().map(|msg| Spans::from(Span::raw(msg))).collect();
    f.render_widget(
        Paragraph::new(Text::from(lines)).block(Block::default().title("MESSAGES").borders(Borders::ALL)),
        rect,
    );
}

pub enum Action {
    StopAndQuit,
    CancelAndQuit, // Stop and delete current recording

    RestartRecording, // Stop and delete current recording, and start a new recording
    StartAnotherRecording,
}

fn get_action(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('q') => Some(Action::StopAndQuit),
        KeyCode::Char('Q') => Some(Action::CancelAndQuit),

        KeyCode::Char(' ') => Some(Action::StartAnotherRecording),
        KeyCode::Backspace => Some(Action::RestartRecording),
        _ => None,
    }
}

pub fn main<B: Backend>(
    terminal: &mut Terminal<B>,
    device: &Device,
    filename: &str,
    messages: &mut Vec<String>,
) -> Result<Action> {
    // Set up the input device and stream with the default input config.
    let config = device
        .default_input_config()
        .expect("Failed to get default input config");

    // The WAV file we're recording to.
    let spec = wav_spec_from_config(&config);
    let writer = hound::WavWriter::create(filename, spec)?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    // Run the input stream on a separate thread.
    let writer_2 = writer.clone();

    let err_fn = move |err| {
        panic!("An error occurred on stream: {}", err);
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data, _: &_| write_input_data::<f32, f32>(data, &writer_2),
            err_fn,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data, _: &_| write_input_data::<i16, i16>(data, &writer_2),
            err_fn,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data, _: &_| write_input_data::<u16, i16>(data, &writer_2),
            err_fn,
        )?,
    };

    stream.play()?;

    // Record until user gives an action
    let action = run_app(
        terminal,
        filename,
        device.name().unwrap_or("Unknown".to_string()),
        messages,
    );

    drop(stream);
    writer.lock().unwrap().take().unwrap().finalize()?;

    action
}

fn sample_format(format: cpal::SampleFormat) -> hound::SampleFormat {
    match format {
        cpal::SampleFormat::U16 => hound::SampleFormat::Int,
        cpal::SampleFormat::I16 => hound::SampleFormat::Int,
        cpal::SampleFormat::F32 => hound::SampleFormat::Float,
    }
}

fn wav_spec_from_config(config: &cpal::SupportedStreamConfig) -> hound::WavSpec {
    hound::WavSpec {
        channels: config.channels() as _,
        sample_rate: config.sample_rate().0 as _,
        bits_per_sample: (config.sample_format().sample_size() * 8) as _,
        sample_format: sample_format(config.sample_format()),
    }
}

type WavWriterHandle = Arc<Mutex<Option<hound::WavWriter<BufWriter<File>>>>>;

fn write_input_data<T, U>(input: &[T], writer: &WavWriterHandle)
where
    T: cpal::Sample,
    U: cpal::Sample + hound::Sample,
{
    if let Ok(mut guard) = writer.try_lock() {
        if let Some(writer) = guard.as_mut() {
            for &sample in input.iter() {
                let sample: U = cpal::Sample::from(&sample);
                writer.write_sample(sample).ok();
            }
        }
    }
}
