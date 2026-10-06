//! Bouncing Ball is a stereo/mono effect that retriggers a hit the way a
//! dropped ball comes to rest.
//!
//! Each bounce keeps a fraction of the previous gap, so the triggers hasten
//! along a geometric curve and finish in a short rattle. Feed it a drum, a
//! noise burst, or a held tone.

mod dsp;
mod editor;

use dsp::{BounceConfig, Direction, Division, Engine, SourceMode, TriggerMode, MAX_BALLS};
use editor::BallEditor;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug::prelude::*;
use nice_plug_egui::{
    create_egui_editor, EguiEditor, EguiEditorState, EguiNiceSettings, RepaintNotifier,
};
use std::sync::Arc;

const WINDOW_SIZE: LogicalSize<f32> = LogicalSize::new(640.0, 640.0);
/// Fixed square. Resizing would throw the ball and the controls out of place.
const RESIZE_HINT: ResizeHint = ResizeHint::non_resizable();

pub struct BouncingBall {
    params: Arc<BouncingBallParams>,
    engine: Engine,
    editor_state: Arc<EguiEditorState>,
    repaint_notifier: RepaintNotifier,
    initial_editor: Option<BallEditor>,
}

#[derive(Params)]
struct BouncingBallParams {
    #[id = "trigger"]
    pub trigger: EnumParam<TriggerMode>,
    #[id = "threshold"]
    pub threshold: FloatParam,
    #[id = "source"]
    pub source: EnumParam<SourceMode>,
    #[id = "direction"]
    pub direction: EnumParam<Direction>,
    #[id = "drop"]
    pub drop: FloatParam,
    #[id = "sync"]
    pub sync: BoolParam,
    #[id = "division"]
    pub division: EnumParam<Division>,
    #[id = "bounciness"]
    pub bounciness: FloatParam,
    #[id = "rest"]
    pub rest: FloatParam,
    #[id = "hit"]
    pub hit: FloatParam,
    #[id = "decay"]
    pub decay: FloatParam,
    #[id = "rattle"]
    pub rattle: FloatParam,
    #[id = "balls"]
    pub balls: IntParam,
    #[id = "spread"]
    pub spread: FloatParam,
    #[id = "damping"]
    pub damping: FloatParam,
    #[id = "loop"]
    pub loop_sequence: BoolParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "output"]
    pub output: FloatParam,
}

impl Default for BouncingBallParams {
    fn default() -> Self {
        Self {
            trigger: EnumParam::new("Trigger", TriggerMode::Transient),
            threshold: FloatParam::new(
                "Threshold",
                -24.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 0.0,
                },
            )
            .with_step_size(0.1)
            .with_unit(" dB"),
            source: EnumParam::new("Source", SourceMode::Retrigger),
            direction: EnumParam::new("Direction", Direction::Fall),
            drop: FloatParam::new(
                "Drop",
                420.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 2_000.0,
                    factor: FloatRange::skew_factor(-1.2),
                },
            )
            .with_step_size(1.0)
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            sync: BoolParam::new("Tempo Sync", false),
            division: EnumParam::new("Division", Division::Quarter),
            bounciness: FloatParam::new(
                "Bounciness",
                0.86,
                FloatRange::Linear {
                    min: 0.5,
                    max: 0.97,
                },
            )
            .with_step_size(0.001)
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            rest: FloatParam::new(
                "Rest",
                8.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 40.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_step_size(0.1)
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            hit: FloatParam::new(
                "Hit",
                85.0,
                FloatRange::Skewed {
                    min: 5.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_step_size(1.0)
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            decay: FloatParam::new(
                "Bounce Decay",
                0.75,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01)
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            rattle: FloatParam::new(
                "Rattle",
                70.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 400.0,
                },
            )
            .with_step_size(1.0)
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            balls: IntParam::new(
                "Balls",
                1,
                IntRange::Linear {
                    min: 1,
                    max: MAX_BALLS as i32,
                },
            ),
            spread: FloatParam::new("Spread", 0.35, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01)
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            damping: FloatParam::new("Damping", 0.35, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01)
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            loop_sequence: BoolParam::new("Loop", false),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_step_size(0.01)
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            output: FloatParam::new(
                "Output",
                util::db_to_gain(0.0),
                FloatRange::gain_range(-36.0, 12.0),
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

impl Default for BouncingBall {
    fn default() -> Self {
        let params = Arc::new(BouncingBallParams::default());
        Self {
            engine: Engine::default(),
            editor_state: EguiEditorState::from_size(WINDOW_SIZE, 1.0),
            repaint_notifier: RepaintNotifier::new(),
            initial_editor: Some(BallEditor::new(params.clone())),
            params,
        }
    }
}

impl BouncingBall {
    fn snapshot(&self, tempo: Option<f64>) -> BounceConfig {
        let sample_rate = self.engine.sample_rate().max(1.0);
        let rest_samples = (self.params.rest.value() * 0.001 * sample_rate).max(1.0);
        let drop_samples = self.drop_samples(sample_rate, tempo, rest_samples);
        let restitution = self.params.bounciness.value().clamp(0.5, 0.97);
        let decay = self.params.decay.value().clamp(0.0, 1.0);
        BounceConfig {
            sample_rate,
            trigger: self.params.trigger.value(),
            source: self.params.source.value(),
            direction: self.params.direction.value(),
            drop_samples,
            rest_samples,
            restitution,
            hit_samples: (self.params.hit.value() * 0.001 * sample_rate).round() as usize,
            amp_mul: (1.0 - decay * (1.0 - restitution)).clamp(0.5, 1.0),
            rattle_samples: (self.params.rattle.value() * 0.001 * sample_rate).max(0.0),
            balls: self.params.balls.value().clamp(1, MAX_BALLS as i32) as usize,
            spread: self.params.spread.value().clamp(0.0, 1.0),
            damping: self.params.damping.value().clamp(0.0, 1.0),
            loop_sequence: self.params.loop_sequence.value(),
            threshold: util::db_to_gain(self.params.threshold.value()),
            velocity: self.engine.velocity(),
        }
    }

    fn drop_samples(&self, sample_rate: f32, tempo: Option<f64>, rest_samples: f32) -> f32 {
        let from_tempo = if self.params.sync.value() {
            tempo
                .filter(|bpm| bpm.is_finite() && *bpm > 1.0)
                .map(|bpm| self.params.division.value().beats() * 60.0 / bpm as f32 * sample_rate)
        } else {
            None
        };
        let from_ms = self.params.drop.value() * 0.001 * sample_rate;
        from_tempo.unwrap_or(from_ms).max(rest_samples + 1.0)
    }
}

impl Plugin for BouncingBall {
    const NAME: &'static str = "Bouncing Ball";
    const VENDOR: &'static str = "Bouncing Ball";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames::const_default(),
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type Editor = EguiEditor<BallEditor>;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_egui_editor(
            self.editor_state.clone(),
            self.repaint_notifier.clone(),
            EguiNiceSettings::new().with_resize_hint(RESIZE_HINT),
            self.initial_editor.take()?,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        // Hit length tops out at 500 ms. Keep a little headroom above that.
        let max_hit = (buffer_config.sample_rate * 0.55) as usize;
        self.engine
            .prepare(buffer_config.sample_rate, max_hit.max(8));
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let tempo = context.transport().tempo;
        let mut next_event = context.next_event();
        let channels = buffer.as_slice();
        let channel_count = channels.len().min(2);
        if channel_count == 0 {
            return ProcessStatus::Normal;
        }
        let sample_count = channels[0].len();

        for sample_id in 0..sample_count {
            while let Some(event) = next_event {
                if event.timing() > sample_id as u32 {
                    break;
                }
                if let NoteEvent::NoteOn { velocity, .. } = event {
                    if self.params.trigger.value() == TriggerMode::Midi {
                        let cfg = self.snapshot(tempo);
                        self.engine.trigger(&cfg, velocity);
                    }
                }
                next_event = context.next_event();
            }

            let mix = self.params.mix.smoothed.next();
            let gain = self.params.output.smoothed.next();
            let frame = if channel_count == 1 {
                let sample = channels[0][sample_id];
                [sample, sample]
            } else {
                [channels[0][sample_id], channels[1][sample_id]]
            };
            let cfg = self.snapshot(tempo);
            let wet = self.engine.process_sample(&frame, &cfg);

            if channel_count == 1 {
                let rendered = 0.5 * (wet[0] + wet[1]);
                channels[0][sample_id] = (frame[0] * (1.0 - mix) + rendered * mix) * gain;
            } else {
                for channel in 0..2 {
                    channels[channel][sample_id] =
                        (frame[channel] * (1.0 - mix) + wet[channel] * mix) * gain;
                }
            }
        }

        if self.params.loop_sequence.value() || self.params.trigger.value() == TriggerMode::Free {
            ProcessStatus::KeepAlive
        } else {
            match self.engine.tail_samples() {
                0 => ProcessStatus::Normal,
                tail => ProcessStatus::Tail(tail),
            }
        }
    }
}

impl ClapPlugin for BouncingBall {
    const CLAP_ID: &'static str = "com.bouncing-ball.bouncing-ball";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Hastening retrigger modeled on a bouncing ball. Gaps shrink geometrically, like the rhythm in Bucephalus Bouncing Ball.",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Delay,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

impl Vst3Plugin for BouncingBall {
    const VST3_CLASS_ID: [u8; 16] = *b"BouncingBall0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Delay];
}

nice_export_clap!(BouncingBall);
nice_export_vst3!(BouncingBall);
