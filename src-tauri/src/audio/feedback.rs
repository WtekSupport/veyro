use std::io::Cursor;

use tracing::warn;

const PTT_START: &[u8] = include_bytes!("../../resources/sounds/ptt_start.ogg");
const PTT_STOP: &[u8] = include_bytes!("../../resources/sounds/ptt_stop.ogg");

fn play(bytes: &'static [u8]) {
    std::thread::spawn(move || {
        if let Err(error) = play_sync(bytes) {
            warn!("ptt feedback sound failed: {error}");
        }
    });
}

fn play_sync(bytes: &'static [u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rodio::{Decoder, OutputStream, Sink};

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;
    let source = Decoder::new(Cursor::new(bytes))?;
    sink.append(source);
    sink.sleep_until_end();
    Ok(())
}

pub fn play_ptt_start() {
    play(PTT_START);
}

pub fn play_ptt_stop() {
    play(PTT_STOP);
}
