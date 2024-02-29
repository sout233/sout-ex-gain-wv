// Forked and modified from: https://github.com/robbert-vdh/nih-plug/tree/master/plugins/examples/gain
use nih_plug::prelude::*;
use nih_plug_webview::*;
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct SoutGainRs {
    params: Arc<GainParams>,
    tempo: f64,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Action {
    Init,
    SetSize { width: u32, height: u32 },
    SetGain { value: f32 },
    SetLength { value: f32 },
    SetPow { value: f32 },
    SetAmount { value: f32 },
}

#[derive(Params)]
struct GainParams {
    #[id = "gain"]
    pub gain: FloatParam,
    gain_value_changed: Arc<AtomicBool>,

    #[id = "lenght"]
    pub length: IntParam,

    #[id = "pump"]
    pub pow: FloatParam,

    #[id = "amount"]
    pub amount: FloatParam,
}

impl Default for SoutGainRs {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),
            tempo: 120.0,
        }
    }
}

impl Default for GainParams {
    fn default() -> Self {
        let gain_value_changed = Arc::new(AtomicBool::new(false));

        let v = gain_value_changed.clone();
        let param_callback = Arc::new(move |_: f32| {
            v.store(true, Ordering::Relaxed);
        });

        Self {
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db())
            .with_callback(param_callback.clone()),
            gain_value_changed,

            pow: FloatParam::new(
                "Pow",
                10.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 20.0,
                },
            ),

            length: IntParam::new("Lenght", 0, IntRange::Linear { min: 0, max: 4 })
                .with_unit(" bar"),

            amount: FloatParam::new("Amount", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
        }
    }
}

impl Plugin for SoutGainRs {
    type BackgroundTask = ();
    type SysExMessage = ();

    const NAME: &'static str = "SoutExGain";
    const VENDOR: &'static str = "sout";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "sout_nantang@outlook.com";

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

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.tempo = context.transport().tempo.expect("err: cannot get tempo");

        for channel_samples in buffer.iter_samples() {
            let gain = self.params.gain.smoothed.next();
            let length = self.params.length.smoothed.next();
            let amount = self.params.amount.smoothed.next();
            let pow = self.params.pow.smoothed.next();

            for sample in channel_samples {
                if length > 0 {
                    let second = context
                        .transport()
                        .pos_seconds()
                        .expect("err: cannot get seconds");
                    let beat = self.tempo / 60.0 * second % length as f64;
                    let final_db = -((beat as f32 + 1.0).powf(-pow)) * 50.0 * amount;
                    *sample *= util::db_to_gain(final_db);
                }
                *sample *= gain;
            }
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let gain_value_changed = self.params.gain_value_changed.clone();
        let editor = WebViewEditor::new(HTMLSource::String(include_str!("gui.html")), (200, 200))
            .with_background_color((150, 150, 150, 255))
            .with_developer_mode(true)
            .with_keyboard_handler(move |event| {
                println!("keyboard event: {event:#?}");
                event.key == Key::Escape
            })
            .with_mouse_handler(|event| match event {
                MouseEvent::DragEntered { .. } => {
                    println!("drag entered");
                    EventStatus::AcceptDrop(DropEffect::Copy)
                }
                MouseEvent::DragMoved { .. } => {
                    println!("drag moved");
                    EventStatus::AcceptDrop(DropEffect::Copy)
                }
                MouseEvent::DragLeft => {
                    println!("drag left");
                    EventStatus::Ignored
                }
                MouseEvent::DragDropped { data, .. } => {
                    if let DropData::Files(files) = data {
                        println!("drag dropped: {:?}", files);
                    }
                    EventStatus::AcceptDrop(DropEffect::Copy)
                }
                _ => EventStatus::Ignored,
            })
            .with_event_loop(move |ctx, setter, window| {
                while let Ok(value) = ctx.next_event() {
                    if let Ok(action) = serde_json::from_value(value) {
                        match action {
                            Action::SetGain { value } => {
                                setter.begin_set_parameter(&params.gain);
                                setter.set_parameter_normalized(&params.gain, value);
                                setter.end_set_parameter(&params.gain);
                            }
                            Action::SetLength { value } => {
                                setter.begin_set_parameter(&params.length);
                                setter.set_parameter(&params.length, value as i32);
                                setter.end_set_parameter(&params.length);
                            }
                            Action::SetPow { value } => {
                                setter.begin_set_parameter(&params.pow);
                                setter.set_parameter_normalized(&params.pow, value);
                                setter.end_set_parameter(&params.pow);
                            }
                            Action::SetAmount { value } => {
                                setter.begin_set_parameter(&params.amount);
                                setter.set_parameter_normalized(&params.amount, value);
                                setter.end_set_parameter(&params.amount);
                            }
                            Action::SetSize { width, height } => {
                                ctx.resize(window, width, height);
                            }
                            Action::Init => {
                                let _ = ctx.send_json(json!({
                                    "type": "set_size",
                                    "width": ctx.width.load(Ordering::Relaxed),
                                    "height": ctx.height.load(Ordering::Relaxed)
                                }));
                            }
                        }
                    } else {
                        panic!("Invalid action received from web UI.")
                    }
                }

                if gain_value_changed.swap(false, Ordering::Relaxed) {
                    let _ = ctx.send_json(json!({
                        "type": "param_change",
                        "param": "gain",
                        "value": params.gain.unmodulated_normalized_value(),
                        "text": params.gain.to_string()
                    }));
                }
            });

        Some(Box::new(editor))
    }

    fn deactivate(&mut self) {}
}

impl ClapPlugin for SoutGainRs {
    const CLAP_ID: &'static str = "org.eu.sout.audio.exgainwv";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("it is a gain plugin written by rust");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for SoutGainRs {
    const VST3_CLASS_ID: [u8; 16] = *b"SoutGainWebView!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_clap!(SoutGainRs);
nih_export_vst3!(SoutGainRs);
