//! Bouncing-ball clock and the audio it drives.
//!
//! A ball that keeps a fraction `r` of its speed on each impact waits
//! `gap, gap·r, gap·r², …` between hits. That is the hastening rhythm: the
//! gaps form a finite geometric series and collapse into a rattle.

use nice_plug::prelude::Enum;
use std::f32::consts::{SQRT_2, TAU};

pub const MAX_BALLS: usize = 4;
const MAX_VOICES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum TriggerMode {
    #[id = "transient"]
    #[name = "Transient"]
    Transient,
    #[id = "midi"]
    #[name = "MIDI"]
    Midi,
    #[id = "free"]
    #[name = "Free"]
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum SourceMode {
    /// Replay the captured hit on every bounce.
    #[id = "retrigger"]
    #[name = "Retrigger"]
    Retrigger,
    /// Open a short envelope on the live input at every bounce.
    #[id = "gate"]
    #[name = "Gate"]
    Gate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum Direction {
    /// Gaps shrink. The ball is falling and coming to rest.
    #[id = "fall"]
    #[name = "Fall"]
    Fall,
    /// Gaps grow. The gesture played in reverse.
    #[id = "rise"]
    #[name = "Rise"]
    Rise,
    /// Shrink to the floor, then grow back out.
    #[id = "round"]
    #[name = "Round Trip"]
    RoundTrip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
pub enum Division {
    #[id = "1/2"]
    #[name = "1/2"]
    Half,
    #[id = "1/4"]
    #[name = "1/4"]
    Quarter,
    #[id = "1/8"]
    #[name = "1/8"]
    Eighth,
    #[id = "1/16"]
    #[name = "1/16"]
    Sixteenth,
    #[id = "1/4d"]
    #[name = "1/4."]
    DottedQuarter,
    #[id = "1/8d"]
    #[name = "1/8."]
    DottedEighth,
}

impl Division {
    pub fn beats(self) -> f32 {
        match self {
            Self::Half => 2.0,
            Self::Quarter => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
            Self::DottedQuarter => 1.5,
            Self::DottedEighth => 0.75,
        }
    }
}

/// Values for one bounce gesture. Read once per trigger for the things that
/// define the phrase, and every bounce for the things that may be automated.
#[derive(Debug, Clone, Copy)]
pub struct BounceConfig {
    pub sample_rate: f32,
    pub trigger: TriggerMode,
    pub source: SourceMode,
    pub direction: Direction,
    pub drop_samples: f32,
    pub rest_samples: f32,
    pub restitution: f32,
    pub hit_samples: usize,
    /// Amplitude multiplier applied after each falling bounce. `1` holds level.
    pub amp_mul: f32,
    pub rattle_samples: f32,
    pub balls: usize,
    pub spread: f32,
    pub damping: f32,
    pub loop_sequence: bool,
    pub threshold: f32,
    pub velocity: f32,
}

impl BounceConfig {
    fn limits(self) -> (f32, f32, f32, f32) {
        let min_gap = self.rest_samples.max(1.0);
        let max_gap = self.drop_samples.max(min_gap + 1.0);
        let restitution = self.restitution.clamp(0.5, 0.97);
        let amp_mul = self.amp_mul.clamp(0.5, 1.0);
        (min_gap, max_gap, restitution, amp_mul)
    }
}

#[derive(Clone, Copy)]
struct Tick {
    amplitude: f32,
    pan: f32,
    /// Samples until the next hit. `0` when this hit ended the phrase.
    wait: f32,
    alive: bool,
}

#[derive(Clone, Copy)]
struct BounceClock {
    gap: f32,
    amplitude: f32,
    falling: bool,
    rattling: bool,
    rattle_left: f32,
    pan_sign: f32,
    alive: bool,
}

impl BounceClock {
    fn arm(index: usize, cfg: &BounceConfig) -> Self {
        let falling = cfg.direction != Direction::Rise;
        let (min_gap, max_gap, _, _) = cfg.limits();
        let gap = if falling { max_gap } else { min_gap };
        let amplitude = if falling {
            1.0
        } else {
            rise_start_amplitude(cfg)
        };
        Self {
            gap,
            amplitude,
            falling,
            rattling: false,
            rattle_left: 0.0,
            pan_sign: if index % 2 == 0 { -1.0 } else { 1.0 },
            alive: true,
        }
    }
}

/// How many times a rising phrase grows before it reaches the drop height.
/// The first hit starts quieter by this many steps so the last hit lands at 1.
fn rise_growths(cfg: &BounceConfig) -> i32 {
    let (min_gap, max_gap, restitution, _) = cfg.limits();
    let mut gap = min_gap;
    let mut growths = 0i32;
    for _ in 0..4096 {
        if gap >= max_gap {
            break;
        }
        growths += 1;
        gap = (gap / restitution).min(max_gap);
    }
    growths
}

fn rise_start_amplitude(cfg: &BounceConfig) -> f32 {
    let (_, _, _, amp_mul) = cfg.limits();
    if amp_mul >= 0.999 {
        return 1.0;
    }
    amp_mul.powi(rise_growths(cfg)).clamp(1.0e-4, 1.0)
}

fn tick(clock: &mut BounceClock, cfg: &BounceConfig) -> Tick {
    let (min_gap, max_gap, restitution, amp_mul) = cfg.limits();
    let amplitude = clock.amplitude * cfg.velocity.clamp(0.0, 1.0);
    let pan = clock.pan_sign * cfg.spread.clamp(0.0, 1.0) * clock.amplitude.clamp(0.0, 1.0);
    clock.pan_sign = -clock.pan_sign;

    let done = Tick {
        amplitude,
        pan,
        wait: 0.0,
        alive: false,
    };

    if clock.rattling {
        if clock.rattle_left <= 0.0 {
            clock.alive = false;
            return done;
        }
        clock.rattle_left -= clock.gap;
        clock.amplitude *= amp_mul;
        let still = clock.rattle_left > 0.0;
        clock.alive = still;
        return Tick {
            amplitude,
            pan,
            wait: if still { clock.gap.max(1.0) } else { 0.0 },
            alive: still,
        };
    }

    let at_floor = clock.falling && clock.gap <= min_gap;
    let at_ceiling = !clock.falling && clock.gap >= max_gap;

    if at_ceiling {
        clock.alive = false;
        return done;
    }

    if at_floor && cfg.direction != Direction::RoundTrip {
        if cfg.rattle_samples <= min_gap {
            clock.alive = false;
            return done;
        }
        clock.rattling = true;
        clock.gap = min_gap;
        clock.rattle_left = cfg.rattle_samples - min_gap;
        clock.amplitude *= amp_mul;
        return Tick {
            amplitude,
            pan,
            wait: min_gap,
            alive: true,
        };
    }

    if at_floor {
        clock.falling = false;
        clock.gap = min_gap;
    }

    let wait = clock.gap.max(1.0);
    if clock.falling {
        clock.gap = (clock.gap * restitution).max(min_gap);
        clock.amplitude *= amp_mul;
    } else {
        clock.gap = (clock.gap / restitution).min(max_gap);
        clock.amplitude = (clock.amplitude / amp_mul).min(1.0);
    }

    Tick {
        amplitude,
        pan,
        wait,
        alive: true,
    }
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    playhead: usize,
    amplitude: f32,
    pan: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            playhead: 0,
            amplitude: 0.0,
            pan: 0.0,
        }
    }
}

struct Ball {
    countdown: f32,
    clock: BounceClock,
    voices: [Voice; MAX_VOICES],
}

impl Ball {
    fn silent() -> Self {
        Self {
            countdown: 0.0,
            clock: BounceClock {
                gap: 1.0,
                amplitude: 1.0,
                falling: true,
                rattling: false,
                rattle_left: 0.0,
                pan_sign: -1.0,
                alive: false,
            },
            voices: [Voice::default(); MAX_VOICES],
        }
    }

    fn arm(index: usize, ball_count: usize, cfg: &BounceConfig) -> Self {
        let stagger = if ball_count <= 1 {
            0.0
        } else {
            cfg.limits().1 * index as f32 / ball_count as f32
        };
        Self {
            // `+ 1` so the first `countdown -= 1` lands on this ball's onset sample.
            countdown: stagger + 1.0,
            clock: BounceClock::arm(index, cfg),
            voices: [Voice::default(); MAX_VOICES],
        }
    }

    fn advance(&mut self, cfg: &BounceConfig) {
        if !self.clock.alive {
            return;
        }
        self.countdown -= 1.0;
        if self.countdown > 0.0 {
            return;
        }
        let fired = tick(&mut self.clock, cfg);
        self.spawn(fired.amplitude, fired.pan);
        if fired.alive {
            self.countdown += fired.wait.max(1.0);
        }
    }

    fn spawn(&mut self, amplitude: f32, pan: f32) {
        let mut free = None;
        let mut oldest = 0usize;
        let mut oldest_pos = 0usize;
        for (index, voice) in self.voices.iter().enumerate() {
            if !voice.active {
                free = Some(index);
                break;
            }
            if voice.playhead >= oldest_pos {
                oldest_pos = voice.playhead;
                oldest = index;
            }
        }
        let slot = free.unwrap_or(oldest);
        self.voices[slot] = Voice {
            active: true,
            playhead: 0,
            amplitude,
            pan,
        };
    }

    fn busy(&self) -> bool {
        self.clock.alive || self.voices.iter().any(|voice| voice.active)
    }
}

pub struct Engine {
    sample_rate: f32,
    capture: [Vec<f32>; 2],
    recorded: usize,
    recording: bool,
    hit_len: usize,
    source: SourceMode,
    spawned_balls: usize,
    balls: [Ball; MAX_BALLS],
    env: f32,
    armed: bool,
    free_latched: bool,
    restart_pending: bool,
    latched_velocity: f32,
    energy: f32,
    lowpass: [f32; 2],
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            capture: [Vec::new(), Vec::new()],
            recorded: 0,
            recording: false,
            hit_len: 1,
            source: SourceMode::Retrigger,
            spawned_balls: 0,
            balls: [
                Ball::silent(),
                Ball::silent(),
                Ball::silent(),
                Ball::silent(),
            ],
            env: 0.0,
            armed: true,
            free_latched: false,
            restart_pending: false,
            latched_velocity: 1.0,
            energy: 0.0,
            lowpass: [0.0; 2],
        }
    }
}

impl Engine {
    pub fn prepare(&mut self, sample_rate: f32, max_hit_samples: usize) {
        self.sample_rate = sample_rate.max(1.0);
        let length = max_hit_samples.max(8);
        for channel in &mut self.capture {
            channel.resize(length, 0.0);
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        self.recorded = 0;
        self.recording = false;
        self.hit_len = 1;
        self.spawned_balls = 0;
        self.env = 0.0;
        self.armed = true;
        self.free_latched = false;
        self.restart_pending = false;
        self.latched_velocity = 1.0;
        self.energy = 0.0;
        self.lowpass = [0.0; 2];
        for channel in &mut self.capture {
            channel.fill(0.0);
        }
        self.balls = [
            Ball::silent(),
            Ball::silent(),
            Ball::silent(),
            Ball::silent(),
        ];
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn velocity(&self) -> f32 {
        self.latched_velocity
    }

    pub fn trigger(&mut self, cfg: &BounceConfig, velocity: f32) {
        self.latched_velocity = velocity.clamp(0.0, 1.0);
        self.start(cfg, cfg.source == SourceMode::Retrigger);
    }

    /// Samples the host should keep pulling after the input goes quiet.
    pub fn tail_samples(&self) -> u32 {
        let mut remain = 0.0f32;
        for ball in &self.balls {
            if ball.clock.alive {
                remain = remain.max(ball.countdown + ball.clock.gap * 48.0 + self.hit_len as f32);
            }
            for voice in &ball.voices {
                if voice.active {
                    remain = remain.max(self.hit_len.saturating_sub(voice.playhead) as f32);
                }
            }
        }
        if self.recording {
            remain = remain.max(self.hit_len.saturating_sub(self.recorded) as f32);
        }
        remain.ceil().clamp(0.0, 8_000_000.0) as u32
    }

    pub fn process_sample(&mut self, input: &[f32], cfg: &BounceConfig) -> [f32; 2] {
        if self.capture[0].is_empty() || cfg.sample_rate < 1.0 {
            return [0.0; 2];
        }

        if cfg.trigger != TriggerMode::Free {
            self.free_latched = false;
        }
        let looping = cfg.loop_sequence || cfg.trigger == TriggerMode::Free;
        if !looping {
            self.restart_pending = false;
        }

        if cfg.trigger == TriggerMode::Free && !self.free_latched {
            self.latched_velocity = 1.0;
            self.start(cfg, cfg.source == SourceMode::Retrigger);
            self.free_latched = true;
        } else if self.restart_pending {
            self.restart_pending = false;
            let record = cfg.trigger == TriggerMode::Free && cfg.source == SourceMode::Retrigger;
            self.start(cfg, record);
        } else {
            self.detect_transient(input, cfg);
        }

        let mut cfg = *cfg;
        cfg.velocity = self.latched_velocity;

        self.write_capture(input);

        let enabled = cfg.balls.clamp(1, MAX_BALLS).min(self.spawned_balls.max(1));
        for (index, ball) in self.balls.iter_mut().enumerate() {
            if index >= enabled || index >= self.spawned_balls {
                ball.clock.alive = false;
                continue;
            }
            ball.advance(&cfg);
        }

        let wet = self.render(input, &cfg);

        if looping && !self.busy() {
            self.restart_pending = true;
        }

        wet
    }

    fn start(&mut self, cfg: &BounceConfig, record: bool) {
        let balls = cfg.balls.clamp(1, MAX_BALLS);
        self.spawned_balls = balls;
        self.source = cfg.source;
        self.hit_len = cfg.hit_samples.clamp(4, self.capture[0].len());
        self.energy = 1.0;
        self.restart_pending = false;
        if record {
            self.recorded = 0;
            self.recording = true;
        } else {
            self.recording = false;
        }
        for (index, ball) in self.balls.iter_mut().enumerate() {
            if index < balls {
                *ball = Ball::arm(index, balls, cfg);
            } else {
                *ball = Ball::silent();
            }
        }
    }

    fn detect_transient(&mut self, input: &[f32], cfg: &BounceConfig) {
        if cfg.trigger != TriggerMode::Transient {
            return;
        }
        let peak = input
            .iter()
            .fold(0.0f32, |level, sample| level.max(sample.abs()));
        let release = (-1.0 / (cfg.sample_rate * 0.03)).exp();
        if peak > self.env {
            self.env = peak;
        } else {
            self.env *= release;
        }
        if self.env < cfg.threshold * 0.45 {
            self.armed = true;
        }
        if self.armed && self.env >= cfg.threshold && cfg.threshold > 0.0 {
            self.armed = false;
            self.latched_velocity = 1.0;
            self.start(cfg, cfg.source == SourceMode::Retrigger);
        }
    }

    fn write_capture(&mut self, input: &[f32]) {
        if !self.recording || self.recorded >= self.hit_len {
            self.recording = false;
            return;
        }
        let left = input.first().copied().unwrap_or(0.0);
        let right = input.get(1).copied().unwrap_or(left);
        self.capture[0][self.recorded] = left;
        self.capture[1][self.recorded] = right;
        self.recorded += 1;
        if self.recorded >= self.hit_len {
            self.recording = false;
        }
    }

    fn render(&mut self, input: &[f32], cfg: &BounceConfig) -> [f32; 2] {
        let mut wet = [0.0f32; 2];
        let mut hottest = 0.0f32;
        let left_in = input.first().copied().unwrap_or(0.0);
        let right_in = input.get(1).copied().unwrap_or(left_in);

        for ball in &mut self.balls {
            for voice in &mut ball.voices {
                if !voice.active {
                    continue;
                }
                if voice.playhead >= self.hit_len {
                    voice.active = false;
                    continue;
                }
                let envelope = hit_envelope(voice.playhead, self.hit_len, cfg.sample_rate);
                let gain = envelope * voice.amplitude;
                hottest = hottest.max(voice.amplitude);
                let (pan_l, pan_r) = pan_gains(voice.pan);
                match self.source {
                    SourceMode::Retrigger => {
                        let index = voice.playhead;
                        let (captured_l, captured_r) = if index < self.recorded {
                            (self.capture[0][index], self.capture[1][index])
                        } else {
                            (0.0, 0.0)
                        };
                        wet[0] += captured_l * gain * pan_l;
                        wet[1] += captured_r * gain * pan_r;
                    }
                    SourceMode::Gate => {
                        wet[0] += left_in * gain * pan_l;
                        wet[1] += right_in * gain * pan_r;
                    }
                }
                voice.playhead += 1;
            }
        }

        if hottest >= self.energy {
            self.energy = hottest;
        } else {
            self.energy *= 0.9995;
        }

        // Full level and zero damping stay unfiltered, so the first impacts keep
        // their attack. The lowpass closes only as later bounces lose energy.
        let closure = cfg.damping.clamp(0.0, 1.0) * (1.0 - self.energy.clamp(0.0, 1.0));
        if closure < 1.0e-3 {
            self.lowpass = wet;
        } else {
            let open = cfg.sample_rate * 0.45;
            let closed = 450.0;
            let cutoff = (closed + (open - closed) * (1.0 - closure)).clamp(40.0, open);
            let coeff = 1.0 - (-TAU * cutoff / cfg.sample_rate).exp();
            for channel in 0..2 {
                self.lowpass[channel] += coeff * (wet[channel] - self.lowpass[channel]);
                wet[channel] = self.lowpass[channel];
            }
        }
        [soft_limit(wet[0]), soft_limit(wet[1])]
    }

    fn busy(&self) -> bool {
        self.recording || self.balls.iter().any(Ball::busy)
    }
}

fn hit_envelope(playhead: usize, hit_len: usize, sample_rate: f32) -> f32 {
    if hit_len == 0 {
        return 0.0;
    }
    let fade_in = 4.min(hit_len).max(1);
    let fade_out = ((sample_rate * 0.0015) as usize).clamp(1, (hit_len / 4).max(1));
    let attack = ((playhead + 1) as f32 / fade_in as f32).min(1.0);
    let remaining = hit_len.saturating_sub(playhead);
    let release = if remaining < fade_out {
        remaining as f32 / fade_out as f32
    } else {
        1.0
    };
    let shaped = playhead as f32 / hit_len as f32;
    attack * release * (-3.2 * shaped).exp()
}

fn pan_gains(pan: f32) -> (f32, f32) {
    let pan = pan.clamp(-1.0, 1.0);
    let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos() * SQRT_2, angle.sin() * SQRT_2)
}

/// Unity at 0 dBFS, then a gentle knee so overlapping bounces don't splat.
fn soft_limit(sample: f32) -> f32 {
    let drive = 0.85;
    (sample * drive).tanh() / drive.tanh()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fall_cfg() -> BounceConfig {
        BounceConfig {
            sample_rate: 48_000.0,
            trigger: TriggerMode::Transient,
            source: SourceMode::Retrigger,
            direction: Direction::Fall,
            drop_samples: 4_800.0,
            rest_samples: 480.0,
            restitution: 0.8,
            hit_samples: 480,
            amp_mul: 1.0,
            rattle_samples: 0.0,
            balls: 1,
            spread: 0.0,
            damping: 0.0,
            loop_sequence: false,
            threshold: 0.2,
            velocity: 1.0,
        }
    }

    fn script(cfg: &BounceConfig) -> Vec<f32> {
        let mut clock = BounceClock::arm(0, cfg);
        let mut waits = Vec::new();
        for _ in 0..20_000 {
            if !clock.alive {
                break;
            }
            let fired = tick(&mut clock, cfg);
            if !fired.alive {
                break;
            }
            assert!(fired.wait >= 1.0);
            waits.push(fired.wait);
            if waits.len() > 10_000 {
                panic!("bounce phrase did not end");
            }
        }
        waits
    }

    #[test]
    fn falling_gaps_shrink_by_restitution() {
        let cfg = fall_cfg();
        let waits = script(&cfg);
        assert!(waits.len() >= 4, "expected several bounces, got {waits:?}");
        assert!((waits[0] - cfg.drop_samples).abs() < 1.0);
        for pair in waits.windows(2) {
            let ratio = pair[1] / pair[0];
            assert!(
                (ratio - cfg.restitution).abs() < 1.0e-3,
                "gap ratio {ratio} from {} -> {}",
                pair[0],
                pair[1]
            );
            assert!(pair[1] < pair[0]);
        }
        assert!(*waits.last().unwrap() > cfg.rest_samples);
    }

    #[test]
    fn rising_gaps_grow_and_finish_at_full_level() {
        let mut cfg = fall_cfg();
        cfg.direction = Direction::Rise;
        cfg.amp_mul = 0.8;
        cfg.drop_samples = 4_800.0;
        cfg.rest_samples = 600.0;
        let waits = script(&cfg);
        assert!(waits.len() >= 3, "{waits:?}");
        assert!((waits[0] - cfg.rest_samples).abs() < 1.0);
        for pair in waits.windows(2) {
            let ratio = pair[1] / pair[0];
            assert!(
                (ratio - 1.0 / cfg.restitution).abs() < 1.0e-3,
                "gap ratio {ratio}"
            );
        }
        assert!(*waits.last().unwrap() < cfg.drop_samples);

        let mut clock = BounceClock::arm(0, &cfg);
        let mut last = 0.0;
        for _ in 0..20_000 {
            if !clock.alive {
                break;
            }
            let fired = tick(&mut clock, &cfg);
            last = fired.amplitude;
            if !fired.alive {
                break;
            }
        }
        assert!(
            (last - 1.0).abs() < 0.02,
            "last rising hit should recover to full level, was {last}"
        );
    }

    #[test]
    fn round_trip_shrinks_then_grows() {
        let mut cfg = fall_cfg();
        cfg.direction = Direction::RoundTrip;
        cfg.drop_samples = 4_000.0;
        cfg.rest_samples = 500.0;
        cfg.restitution = 0.5;
        let waits = script(&cfg);
        let (min_index, _) = waits
            .iter()
            .enumerate()
            .min_by(|left, right| left.1.total_cmp(right.1))
            .unwrap();
        assert!(min_index > 0 && min_index < waits.len() - 1, "{waits:?}");
        assert!(waits[..min_index]
            .windows(2)
            .all(|pair| pair[1] <= pair[0] + 1.0e-2));
        assert!(waits[min_index..]
            .windows(2)
            .all(|pair| pair[1] + 1.0e-2 >= pair[0]));
    }

    #[test]
    fn phrases_stay_finite_at_the_liveliest_settings() {
        let mut cfg = fall_cfg();
        cfg.restitution = 0.97;
        cfg.drop_samples = 96_000.0;
        cfg.rest_samples = 48.0;
        cfg.rattle_samples = 4_800.0;
        for direction in [Direction::Fall, Direction::Rise, Direction::RoundTrip] {
            cfg.direction = direction;
            let waits = script(&cfg);
            assert!(!waits.is_empty());
            assert!(waits.len() < 20_000);
        }
    }

    #[test]
    fn retrigger_onsets_follow_the_clock() {
        let mut cfg = fall_cfg();
        cfg.drop_samples = 3_840.0;
        cfg.rest_samples = 720.0;
        cfg.hit_samples = 480;
        cfg.restitution = 0.75;
        let expected = script(&cfg);

        let mut engine = Engine::default();
        engine.prepare(cfg.sample_rate, 8_000);
        let mut audio = Vec::new();
        for sample in 0..(cfg.sample_rate as usize) {
            let input = if sample < cfg.hit_samples { 1.0 } else { 0.0 };
            let wet = engine.process_sample(&[input, input], &cfg);
            audio.push(wet[0]);
            assert!(wet[0].is_finite() && wet[1].is_finite());
        }

        let onsets = rising_edges(&audio, 0.15);
        assert!(
            onsets.len() >= expected.len(),
            "onsets {onsets:?} expected gaps {expected:?}"
        );
        for (index, gap) in expected.iter().enumerate() {
            let measured = onsets[index + 1] - onsets[index];
            assert!(
                (measured as f32 - gap).abs() <= 2.0,
                "onset gap {measured} vs clock {gap} (onsets {onsets:?})"
            );
        }
    }

    #[test]
    fn gate_chops_a_held_tone_into_hastening_pulses() {
        let mut cfg = fall_cfg();
        cfg.source = SourceMode::Gate;
        cfg.drop_samples = 2_400.0;
        cfg.rest_samples = 480.0;
        cfg.hit_samples = 200;
        cfg.restitution = 0.7;
        let expected = script(&cfg);

        let mut engine = Engine::default();
        engine.prepare(cfg.sample_rate, 8_000);
        let mut audio = Vec::new();
        for _ in 0..(cfg.sample_rate as usize) {
            let wet = engine.process_sample(&[1.0, 1.0], &cfg);
            audio.push(wet[0]);
        }
        let onsets = rising_edges(&audio, 0.15);
        assert!(onsets.len() > expected.len() / 2);
        let measured = onsets[1] - onsets[0];
        assert!((measured as f32 - expected[0]).abs() <= 2.0);
        assert!(onsets[2] - onsets[1] < measured);
    }

    #[test]
    fn silence_does_not_invent_a_bounce() {
        let cfg = fall_cfg();
        let mut engine = Engine::default();
        engine.prepare(cfg.sample_rate, 8_000);
        for _ in 0..2_000 {
            let wet = engine.process_sample(&[0.0, 0.0], &cfg);
            assert_eq!(wet, [0.0, 0.0]);
        }
    }

    fn rising_edges(audio: &[f32], threshold: f32) -> Vec<usize> {
        let mut edges = Vec::new();
        let mut hot = false;
        for (index, sample) in audio.iter().enumerate() {
            let above = *sample > threshold;
            if above && !hot {
                edges.push(index);
            }
            hot = above;
        }
        edges
    }
}
