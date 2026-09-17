// VAD segmentation tests live in the vad module unit tests.
// This integration test file validates the public module graph compiles for tests.

#[test]
fn vad_config_defaults_are_sane() {
    let config = veyro_lib::vad::VadConfig::default();
    assert!(config.pre_speech_buffer_ms > 0);
    assert!(config.silence_timeout_ms > config.minimum_speech_ms);
}
