//! Microphone capture on a dedicated thread (cpal streams are not `Send`).
//!
//! Opening a CoreAudio input takes ~0.5–1 s, far too slow for push-to-talk. So the stream is
//! built once up front and kept *paused* (no device I/O, no mic indicator); a session just
//! un-pauses it, which takes a few milliseconds.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;

pub type LevelFn = Arc<dyn Fn(f32) + Send + Sync>;
pub type ErrorFn = Box<dyn FnOnce(String) + Send>;

pub struct Recording {
    /// Mono samples at `rate`.
    pub samples: Vec<f32>,
    pub rate: u32,
}

enum Cmd {
    /// Build (or rebuild) the paused stream for this device ahead of time.
    Prepare { device: Option<String> },
    Start { device: Option<String>, on_level: LevelFn, on_error: ErrorFn },
    Stop { reply: Sender<Option<Recording>> },
}

struct Prepared {
    stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    level: Arc<Mutex<Option<LevelFn>>>,
    active: Arc<AtomicBool>,
    rate: u32,
    /// Requested device (None = system default) and the concrete device name it resolved to.
    requested: Option<String>,
    resolved: String,
}

#[derive(Clone)]
pub struct Recorder {
    tx: Sender<Cmd>,
}

impl Recorder {
    pub fn spawn() -> Self {
        let (tx, rx) = channel::<Cmd>();
        std::thread::Builder::new()
            .name("sori-recorder".into())
            .spawn(move || {
                let mut prepared: Option<Prepared> = None;
                let mut running = false;
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        Cmd::Prepare { device } => {
                            if !running && !is_current(&prepared, device.as_deref()) {
                                prepared = None;
                                let t = Instant::now();
                                match build(device.as_deref()) {
                                    Ok(p) => {
                                        log::info!("mic prepared: {} @{}Hz in {}ms", p.resolved, p.rate, t.elapsed().as_millis());
                                        prepared = Some(p);
                                    }
                                    Err(e) => log::warn!("mic prepare failed: {e:#}"),
                                }
                            }
                        }
                        Cmd::Start { device, on_level, on_error } => {
                            let t = Instant::now();
                            if !is_current(&prepared, device.as_deref()) {
                                prepared = None;
                                match build(device.as_deref()) {
                                    Ok(p) => prepared = Some(p),
                                    Err(e) => {
                                        on_error(e.to_string());
                                        continue;
                                    }
                                }
                            }
                            let p = prepared.as_ref().unwrap();
                            p.buf.lock().clear();
                            *p.level.lock() = Some(on_level);
                            p.active.store(true, Ordering::SeqCst);
                            if let Err(e) = p.stream.play() {
                                log::warn!("mic play failed ({e}); rebuilding");
                                prepared = None;
                                match build(device.as_deref()).and_then(|p| {
                                    p.active.store(true, Ordering::SeqCst);
                                    p.stream.play()?;
                                    Ok(p)
                                }) {
                                    Ok(p) => prepared = Some(p),
                                    Err(e) => {
                                        on_error(e.to_string());
                                        continue;
                                    }
                                }
                            }
                            running = true;
                            log::info!("mic started in {}ms", t.elapsed().as_millis());
                        }
                        Cmd::Stop { reply } => {
                            let rec = if running {
                                prepared.as_ref().map(|p| {
                                    let _ = p.stream.pause();
                                    p.active.store(false, Ordering::SeqCst);
                                    *p.level.lock() = None;
                                    Recording { samples: std::mem::take(&mut *p.buf.lock()), rate: p.rate }
                                })
                            } else {
                                None
                            };
                            running = false;
                            let _ = reply.send(rec);
                        }
                    }
                }
            })
            .expect("spawn recorder");
        Self { tx }
    }

    /// Build the paused stream now so the next `start` is instant.
    pub fn prepare(&self, device: Option<String>) {
        let _ = self.tx.send(Cmd::Prepare { device });
    }

    /// Non-blocking: the recorder thread un-pauses the stream; failures go to `on_error`.
    pub fn start(&self, device: Option<String>, on_level: LevelFn, on_error: ErrorFn) {
        let _ = self.tx.send(Cmd::Start { device, on_level, on_error });
    }

    /// Blocks until the recorder has processed any pending start, then returns the audio.
    pub fn stop(&self) -> Option<Recording> {
        let (reply, rx) = channel();
        self.tx.send(Cmd::Stop { reply }).ok()?;
        rx.recv().ok().flatten()
    }
}

pub fn list_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|it| it.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// Is the prepared stream still the right one (same requested device, and for the system
/// default: the default hasn't changed, e.g. AirPods connected)?
fn is_current(p: &Option<Prepared>, requested: Option<&str>) -> bool {
    let Some(p) = p else { return false };
    if p.requested.as_deref() != requested {
        return false;
    }
    match requested {
        Some(_) => true,
        None => cpal::default_host()
            .default_input_device()
            .and_then(|d| d.name().ok())
            .map_or(false, |n| n == p.resolved),
    }
}

fn build(device_name: Option<&str>) -> Result<Prepared> {
    let host = cpal::default_host();
    let device = match device_name {
        Some(name) => host
            .input_devices()?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .or_else(|| host.default_input_device()),
        None => host.default_input_device(),
    }
    .ok_or_else(|| anyhow!("Microphone not found"))?;
    let resolved = device.name().unwrap_or_default();
    let config = device.default_input_config()?;
    let rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let buf = Arc::new(Mutex::new(Vec::<f32>::with_capacity(rate as usize * 60)));
    let level: Arc<Mutex<Option<LevelFn>>> = Arc::new(Mutex::new(None));
    let active = Arc::new(AtomicBool::new(false));
    let err_fn = |e| log::error!("audio stream error: {e}");

    let sink = {
        let buf = buf.clone();
        let level = level.clone();
        let active = active.clone();
        let mut last_emit = Instant::now();
        let mut pending: Vec<f32> = Vec::new();
        move |mono: &[f32]| {
            if !active.load(Ordering::Relaxed) {
                return;
            }
            buf.lock().extend_from_slice(mono);
            pending.extend_from_slice(mono);
            if last_emit.elapsed().as_millis() >= 33 {
                if let Some(cb) = level.lock().as_ref() {
                    cb(sori_core::audio::level(&pending));
                }
                pending.clear();
                last_emit = Instant::now();
            }
        }
    };

    let stream_config: cpal::StreamConfig = config.clone().into();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let mut sink = sink;
            device.build_input_stream(&stream_config, move |data: &[f32], _| sink(&downmix(data.iter().copied(), channels)), err_fn, None)?
        }
        cpal::SampleFormat::I16 => {
            let mut sink = sink;
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| sink(&downmix(data.iter().map(|s| *s as f32 / i16::MAX as f32), channels)),
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::I32 => {
            let mut sink = sink;
            device.build_input_stream(
                &stream_config,
                move |data: &[i32], _| sink(&downmix(data.iter().map(|s| *s as f32 / i32::MAX as f32), channels)),
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let mut sink = sink;
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| sink(&downmix(data.iter().map(|s| (*s as f32 - 32768.0) / 32768.0), channels)),
                err_fn,
                None,
            )?
        }
        f => return Err(anyhow!("Unsupported audio format: {f:?}")),
    };
    // cpal may auto-start on some hosts; make sure we're idle until a session begins.
    let _ = stream.pause();
    Ok(Prepared { stream, buf, level, active, rate, requested: device_name.map(str::to_string), resolved })
}

fn downmix(it: impl Iterator<Item = f32>, channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return it.collect();
    }
    let v: Vec<f32> = it.collect();
    v.chunks(channels).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect()
}
