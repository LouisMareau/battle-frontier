//! Sound: the props' and creatures' sounds and the places' ambience looping where they
//! are, and one-shots (a script's `sound`, a creature's cry). One mixer for the whole game:
//! each frame it's told what should be playing (`Want`s, each with a key that lasts) and
//! where the listener stands, and it starts what's new, stops what's gone and sets every
//! loudness by distance. Files come through `Files` (fetched in the browser, read on the
//! desktop). In the browser, Web Audio, which plays nothing before the first click or key
//! (`unlock`); on the desktop, rodio.

use std::collections::HashMap;

use glam::Vec3;

use crate::platform::{Files, Load};

/// A sound that should be playing, looping: at `pos` (or everywhere, when None), `volume`
/// up close, fading to nothing `range` cells away.
#[derive(Clone, Debug, PartialEq)]
pub struct Want {
    pub key: String,
    pub file: String,
    pub pos: Option<Vec3>,
    pub volume: f32,
    pub range: f32,
}

/// How loud a sound is from `range` cells away: full up close, gone at the edge.
pub fn level(listener: Vec3, pos: Option<Vec3>, volume: f32, range: f32) -> f32 {
    let k = match pos {
        Some(p) if range > 0.0 => (1.0 - listener.distance(p) / range).clamp(0.0, 1.0).powf(1.5),
        Some(_) => 0.0,
        None => 1.0,
    };
    (volume * k).clamp(0.0, 4.0)
}

/// A one-shot asked for before its file was in: tried again until it is (or too late).
struct Shot {
    file: String,
    gain: f32,
    since: f64,
}

pub struct Audio {
    backend: Backend,
    /// the loops playing, by key: their file (a file changing restarts the loop)
    loops: HashMap<String, String>,
    pending: Vec<Shot>,
    pub master: f32,
    pub enabled: bool,
    listener: Vec3,
}

impl Default for Audio {
    fn default() -> Self {
        Audio::new()
    }
}

impl Audio {
    pub fn new() -> Audio {
        Audio { backend: Backend::new(), loops: HashMap::new(), pending: vec![], master: 1.0, enabled: true, listener: Vec3::ZERO }
    }

    /// After a click or a key: browsers play nothing before one.
    pub fn unlock(&mut self) {
        self.backend.unlock();
    }

    /// "off" (no sound device), "locked" (waiting for a click or key) or "on".
    pub fn state(&self) -> &'static str {
        self.backend.state()
    }

    pub fn playing(&self) -> usize {
        self.loops.len()
    }

    /// What should be playing now, from where it's heard.
    pub fn update(&mut self, files: &Files, listener: Vec3, wants: &[Want], now: f64) {
        self.listener = listener;
        let keep: Vec<&str> = if self.enabled { wants.iter().map(|w| w.key.as_str()).collect() } else { vec![] };
        let gone: Vec<String> = self.loops.keys().filter(|k| !keep.contains(&k.as_str())).cloned().collect();
        for k in gone {
            self.backend.stop(&k);
            self.loops.remove(&k);
        }
        if !self.enabled {
            self.pending.clear();
            return;
        }
        for w in wants {
            let gain = level(listener, w.pos, w.volume, w.range) * self.master;
            match self.loops.get(&w.key) {
                Some(file) if *file == w.file => self.backend.set_gain(&w.key, gain),
                _ => {
                    if let Load::Ready(bytes) = files.get(&w.file) {
                        if self.backend.start_loop(&w.key, &w.file, &bytes, gain) {
                            self.loops.insert(w.key.clone(), w.file.clone());
                        }
                    }
                }
            }
        }
        // one-shots whose file wasn't in yet
        let mut still = vec![];
        for s in self.pending.drain(..) {
            match files.get(&s.file) {
                Load::Ready(bytes) => self.backend.play_once(&s.file, &bytes, s.gain),
                Load::Failed(_) => {}
                Load::Pending if now - s.since < 5000.0 => still.push(s),
                Load::Pending => {}
            }
        }
        self.pending = still;
        self.backend.tick(now);
    }

    /// A sound once, now: at `pos` (or right here), `volume` up close.
    pub fn play_once(&mut self, files: &Files, file: &str, pos: Option<Vec3>, volume: f32, range: f32, now: f64) {
        if !self.enabled {
            return;
        }
        let gain = level(self.listener, pos, volume, range) * self.master;
        if gain <= 0.0 {
            return;
        }
        match files.get(file) {
            Load::Ready(bytes) => self.backend.play_once(file, &bytes, gain),
            Load::Pending => self.pending.push(Shot { file: file.to_string(), gain, since: now }),
            Load::Failed(_) => {}
        }
    }

    /// Everything stops (another screen).
    pub fn stop_all(&mut self) {
        for k in self.loops.keys() {
            self.backend.stop(k);
        }
        self.loops.clear();
        self.pending.clear();
    }
}

// ---------------------------------------------------------------- the desktop: rodio

#[cfg(not(target_arch = "wasm32"))]
struct Backend {
    device: Option<rodio::MixerDeviceSink>,
    loops: HashMap<String, rodio::Player>,
    shots: Vec<(rodio::Player, f64)>,
    /// web time of the last tick, to let one-shots go
    now: f64,
}

#[cfg(not(target_arch = "wasm32"))]
impl Backend {
    fn new() -> Backend {
        let device = match rodio::DeviceSinkBuilder::open_default_sink() {
            Ok(d) => Some(d),
            Err(e) => {
                log::warn!("no sound device: {e}");
                None
            }
        };
        Backend { device, loops: HashMap::new(), shots: vec![], now: 0.0 }
    }

    fn unlock(&mut self) {}

    fn state(&self) -> &'static str {
        if self.device.is_some() { "on" } else { "off" }
    }

    fn decoder(bytes: &[u8]) -> Option<rodio::Decoder<std::io::Cursor<Vec<u8>>>> {
        match rodio::Decoder::new(std::io::Cursor::new(bytes.to_vec())) {
            Ok(d) => Some(d),
            Err(e) => {
                log::warn!("a sound that won't decode: {e}");
                None
            }
        }
    }

    fn start_loop(&mut self, key: &str, _file: &str, bytes: &[u8], gain: f32) -> bool {
        use rodio::Source;
        let Some(device) = &self.device else { return false };
        let Some(decoder) = Self::decoder(bytes) else { return false };
        let player = rodio::Player::connect_new(device.mixer());
        player.set_volume(gain);
        player.append(decoder.repeat_infinite());
        if let Some(old) = self.loops.insert(key.to_string(), player) {
            old.stop();
        }
        true
    }

    fn set_gain(&mut self, key: &str, gain: f32) {
        if let Some(p) = self.loops.get(key) {
            p.set_volume(gain);
        }
    }

    fn stop(&mut self, key: &str) {
        if let Some(p) = self.loops.remove(key) {
            p.stop();
        }
    }

    fn play_once(&mut self, _file: &str, bytes: &[u8], gain: f32) {
        use rodio::Source;
        let Some(device) = &self.device else { return };
        let Some(decoder) = Self::decoder(bytes) else { return };
        let seconds = decoder.total_duration().map(|d| d.as_secs_f64()).unwrap_or(10.0);
        let player = rodio::Player::connect_new(device.mixer());
        player.set_volume(gain);
        player.append(decoder);
        self.shots.push((player, self.now + seconds * 1000.0 + 500.0));
    }

    fn tick(&mut self, now: f64) {
        self.now = now;
        self.shots.retain(|(p, until)| now < *until && !p.empty());
    }
}

// ---------------------------------------------------------------- the browser: Web Audio

/// Decoded files, or why not; None while one decodes.
#[cfg(target_arch = "wasm32")]
type Buffers = std::rc::Rc<std::cell::RefCell<HashMap<String, Option<Result<web_sys::AudioBuffer, String>>>>>;

#[cfg(target_arch = "wasm32")]
struct Backend {
    ctx: Option<web_sys::AudioContext>,
    buffers: Buffers,
    loops: HashMap<String, (web_sys::AudioBufferSourceNode, web_sys::GainNode)>,
    shots: Vec<(web_sys::AudioBufferSourceNode, f64)>,
    now: f64,
}

#[cfg(target_arch = "wasm32")]
impl Backend {
    fn new() -> Backend {
        let ctx = match web_sys::AudioContext::new() {
            Ok(c) => Some(c),
            Err(e) => {
                log::warn!("no Web Audio: {e:?}");
                None
            }
        };
        Backend { ctx, buffers: Default::default(), loops: HashMap::new(), shots: vec![], now: 0.0 }
    }

    fn unlock(&mut self) {
        if let Some(ctx) = &self.ctx {
            if ctx.state() == web_sys::AudioContextState::Suspended {
                let _ = ctx.resume();
            }
        }
    }

    fn state(&self) -> &'static str {
        match &self.ctx {
            None => "off",
            Some(c) if c.state() == web_sys::AudioContextState::Running => "on",
            Some(_) => "locked",
        }
    }

    /// The decoded file, decoding it first (asynchronously) when it's new.
    fn buffer(&mut self, file: &str, bytes: &[u8]) -> Option<web_sys::AudioBuffer> {
        let Some(ctx) = &self.ctx else { return None };
        let mut buffers = self.buffers.borrow_mut();
        match buffers.get(file) {
            Some(Some(Ok(b))) => return Some(b.clone()),
            Some(_) => return None,
            None => {}
        }
        buffers.insert(file.to_string(), None);
        drop(buffers);
        let array = js_sys::Uint8Array::from(bytes);
        let Ok(promise) = ctx.decode_audio_data(&array.buffer()) else {
            self.buffers.borrow_mut().insert(file.to_string(), Some(Err("decode refused".into())));
            return None;
        };
        let cache = self.buffers.clone();
        let key = file.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            use wasm_bindgen::JsCast;
            let result = match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(v) => v.dyn_into::<web_sys::AudioBuffer>().map_err(|_| "not an audio buffer".to_string()),
                Err(e) => Err(format!("{e:?}")),
            };
            if let Err(e) = &result {
                log::warn!("{key}: {e}");
            }
            cache.borrow_mut().insert(key, Some(result));
        });
        None
    }

    fn voice(&self, buffer: &web_sys::AudioBuffer, gain: f32, looping: bool) -> Option<(web_sys::AudioBufferSourceNode, web_sys::GainNode)> {
        let ctx = self.ctx.as_ref()?;
        let source = ctx.create_buffer_source().ok()?;
        source.set_buffer(Some(buffer));
        source.set_loop(looping);
        let g = ctx.create_gain().ok()?;
        g.gain().set_value(gain);
        source.connect_with_audio_node(&g).ok()?;
        g.connect_with_audio_node(&ctx.destination()).ok()?;
        web_sys::AudioScheduledSourceNode::start(&source).ok()?;
        Some((source, g))
    }

    fn start_loop(&mut self, key: &str, file: &str, bytes: &[u8], gain: f32) -> bool {
        let Some(buffer) = self.buffer(file, bytes) else { return false };
        let Some(v) = self.voice(&buffer, gain, true) else { return false };
        if let Some((old, _)) = self.loops.insert(key.to_string(), v) {
            let _ = web_sys::AudioScheduledSourceNode::stop(&old);
        }
        true
    }

    fn set_gain(&mut self, key: &str, gain: f32) {
        if let (Some(ctx), Some((_, g))) = (&self.ctx, self.loops.get(key)) {
            // eased over 50 ms: no clicks as things move
            let _ = g.gain().set_target_at_time(gain, ctx.current_time(), 0.05);
        }
    }

    fn stop(&mut self, key: &str) {
        if let Some((source, _)) = self.loops.remove(key) {
            let _ = web_sys::AudioScheduledSourceNode::stop(&source);
        }
    }

    fn play_once(&mut self, file: &str, bytes: &[u8], gain: f32) {
        let Some(buffer) = self.buffer(file, bytes) else { return };
        if let Some((source, _)) = self.voice(&buffer, gain, false) {
            self.shots.push((source, self.now + buffer.duration() * 1000.0 + 500.0));
        }
    }

    fn tick(&mut self, now: f64) {
        self.now = now;
        self.shots.retain(|(_, until)| now < *until);
    }
}
