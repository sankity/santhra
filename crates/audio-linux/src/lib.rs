use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use anyhow::{Result, anyhow};
use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use spa::param::audio::{AudioFormat, AudioInfoRaw};
use spa::param::format::{MediaSubtype, MediaType};
use spa::param::format_utils;

pub const SAMPLE_RATE: u32 = 16_000;
const QUEUE_CHUNKS: usize = 64;

pub struct Capture {
    pub rx: mpsc::Receiver<Vec<f32>>,
    pub dropped: Arc<AtomicU64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sink {
    pub name: String,
    pub description: String,
}

pub fn spawn(target: Option<String>, stop: Arc<AtomicBool>) -> Result<Capture> {
    let (tx, rx) = mpsc::sync_channel(QUEUE_CHUNKS);
    let dropped = Arc::new(AtomicU64::new(0));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    let thread_dropped = dropped.clone();
    std::thread::Builder::new()
        .name("pipewire-capture".into())
        .spawn(move || {
            if let Err(err) = run(tx, thread_dropped, target, stop, ready_tx.clone()) {
                let _ = ready_tx.send(Err(err.to_string()));
            }
        })?;

    ready_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| anyhow!("pipewire capture did not start within 5 s"))?
        .map_err(|err| anyhow!("pipewire: {err}"))?;
    Ok(Capture { rx, dropped })
}

fn run(
    tx: mpsc::SyncSender<Vec<f32>>,
    dropped: Arc<AtomicU64>,
    target: Option<String>,
    stop: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<(), String>>,
) -> Result<(), pw::Error> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;

    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::STREAM_CAPTURE_SINK => "true",
    };
    if let Some(target) = target {
        props.insert(*pw::keys::TARGET_OBJECT, target);
    }

    let stream = pw::stream::StreamBox::new(&core, "voxera-capture", props)?;

    let _listener = stream
        .add_local_listener_with_user_data(AudioInfoRaw::default())
        .param_changed(|_, info, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = format_utils::parse_format(param) else {
                return;
            };
            if media_type == MediaType::Audio && media_subtype == MediaSubtype::Raw {
                let _ = info.parse(param);
            }
        })
        .process(move |stream, info| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let size = data.chunk().size() as usize;
            let Some(bytes) = data.data() else { return };
            let samples = to_mono(
                &bytes[..size.min(bytes.len())],
                info.channels().max(1) as usize,
            );
            if tx.try_send(samples).is_err() {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
        })
        .register()?;

    let mut audio_info = AudioInfoRaw::new();
    audio_info.set_format(AudioFormat::F32LE);
    audio_info.set_rate(SAMPLE_RATE);
    audio_info.set_channels(1);
    let object = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(object),
    )
    .expect("serialize audio format")
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&values).expect("valid audio format pod")];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    ready.send(Ok(())).ok();
    while !stop.load(Ordering::Relaxed) {
        mainloop
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(200)));
    }
    Ok(())
}

pub fn list_sinks() -> Vec<Sink> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(enumerate_sinks());
    });
    rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default()
}

fn enumerate_sinks() -> Vec<Sink> {
    let Ok(mainloop) = pw::main_loop::MainLoopRc::new(None) else {
        return Vec::new();
    };
    let Ok(context) = pw::context::ContextRc::new(&mainloop, None) else {
        return Vec::new();
    };
    let Ok(core) = context.connect_rc(None) else {
        return Vec::new();
    };
    let Ok(registry) = core.get_registry() else {
        return Vec::new();
    };
    let Ok(pending) = core.sync(0) else {
        return Vec::new();
    };

    let sinks = Rc::new(RefCell::new(Vec::new()));
    let done = Rc::new(Cell::new(false));

    let _core_listener = core
        .add_listener_local()
        .done({
            let done = done.clone();
            let mainloop = mainloop.clone();
            move |id, seq| {
                if id == pw::core::PW_ID_CORE && seq == pending {
                    done.set(true);
                    mainloop.quit();
                }
            }
        })
        .register();

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let sinks = sinks.clone();
            move |global| {
                let Some(props) = global.props else { return };
                if props.get(*pw::keys::MEDIA_CLASS) != Some("Audio/Sink") {
                    return;
                }
                let Some(name) = props.get(*pw::keys::NODE_NAME) else {
                    return;
                };
                let description = props
                    .get(*pw::keys::NODE_DESCRIPTION)
                    .or_else(|| props.get(*pw::keys::NODE_NICK))
                    .unwrap_or(name);
                sinks.borrow_mut().push(Sink {
                    name: name.to_string(),
                    description: description.to_string(),
                });
            }
        })
        .register();

    mainloop.run();
    sinks.borrow().clone()
}

pub fn to_mono(bytes: &[u8], channels: usize) -> Vec<f32> {
    let samples: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    let channels = channels.max(1);
    if channels == 1 {
        return samples;
    }
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f32_bytes(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn to_mono_passthrough_and_average() {
        assert_eq!(to_mono(&f32_bytes(&[0.5, -0.5]), 1), vec![0.5, -0.5]);
        let stereo = f32_bytes(&[0.5, -0.5, 1.0, 0.0]);
        assert_eq!(to_mono(&stereo, 2), vec![0.0, 0.5]);
    }

    #[test]
    fn to_mono_ignores_trailing_partial_sample() {
        assert_eq!(
            to_mono(&[1, 2, 3, 4, 5], 1),
            vec![f32::from_le_bytes([1, 2, 3, 4])]
        );
    }

    #[test]
    #[ignore = "needs a running PipeWire session"]
    fn lists_sinks() {
        let sinks = list_sinks();
        assert!(!sinks.is_empty(), "no Audio/Sink found");
        assert!(
            sinks
                .iter()
                .all(|sink| !sink.name.is_empty() && !sink.description.is_empty())
        );
    }
}
