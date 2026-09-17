use veyro_lib::app::state::AppState;

#[test]
fn ready_to_listening_to_processing_is_valid() {
    assert!(AppState::Ready.can_transition_to(AppState::Listening));
    assert!(AppState::Listening.can_transition_to(AppState::Processing));
    assert!(AppState::Processing.can_transition_to(AppState::Transcribing));
}

#[test]
fn disabled_cannot_skip_to_injecting() {
    assert!(!AppState::Disabled.can_transition_to(AppState::Injecting));
}

#[test]
fn network_error_recovers_to_ready() {
    assert!(AppState::NetworkUnavailable.can_transition_to(AppState::Ready));
}
