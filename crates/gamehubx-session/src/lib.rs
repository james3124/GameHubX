//! R1 session ownership contract.
//!
//! This crate deliberately owns lifecycle state and cancellation only. Process
//! spawning, native renderer handles and JNI/Android bindings are added behind
//! this contract in later migration phases.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub mod assets;
pub mod audio;
pub mod container;
pub mod display;
pub mod input;
pub mod launch;

pub use assets::{AssetError, RuntimeAssets};
pub use audio::{AudioError, AudioMode, AudioPlan, AudioProcess};
pub use container::{WinlatorContainer, WinlatorContainers};
pub use display::{
    DisplayBackend, DisplayBridge, DisplayError, DisplayState, Generation, NativeHandle,
};
pub use input::{Drain, Event, InputRouter, PushOutcome, EVENT_CAPACITY};
pub use launch::{
    LaunchError, SessionBuildError, SessionLaunch, SessionProcess, WineLaunch, WineProcess,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRequest {
    pub game_id: u64,
    pub exe: PathBuf,
    pub container: PathBuf,
    pub runtime: PathBuf,
    pub memory_limit_mb: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    Starting,
    Running,
    Stopping,
    Exited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    InvalidTransition,
}

pub struct Session {
    request: SessionRequest,
    state: SessionState,
    cancel: Arc<AtomicBool>,
}

impl Session {
    pub fn request(&self) -> &SessionRequest {
        &self.request
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    fn begin(&mut self) -> Result<(), SessionError> {
        if self.state != SessionState::Idle {
            return Err(SessionError::InvalidTransition);
        }
        self.state = SessionState::Starting;
        self.state = SessionState::Running;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), SessionError> {
        if !matches!(self.state, SessionState::Starting | SessionState::Running) {
            return Err(SessionError::InvalidTransition);
        }
        self.state = SessionState::Stopping;
        self.cancel.store(true, Ordering::Release);
        self.state = SessionState::Exited;
        Ok(())
    }
}

/// Owns exactly one session. It is intentionally not Clone; dropping it signals
/// cancellation and leaves later phases responsible for joining process/native
/// cleanup before the guard is dropped.
pub struct SessionGuard {
    session: Option<Session>,
}

impl SessionGuard {
    pub fn start(request: SessionRequest) -> Result<Self, SessionError> {
        let mut session = Session {
            request,
            state: SessionState::Idle,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        session.begin()?;
        Ok(Self {
            session: Some(session),
        })
    }

    pub fn state(&self) -> SessionState {
        self.session
            .as_ref()
            .map(Session::state)
            .unwrap_or(SessionState::Exited)
    }

    pub fn request(&self) -> Option<&SessionRequest> {
        self.session.as_ref().map(Session::request)
    }

    pub fn cancel_token(&self) -> Option<Arc<AtomicBool>> {
        self.session.as_ref().map(Session::cancel_token)
    }

    pub fn stop(&mut self) -> Result<(), SessionError> {
        match self.session.as_mut() {
            Some(session) => session.stop(),
            None => Err(SessionError::InvalidTransition),
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Some(session) = self.session.as_mut() {
            if matches!(
                session.state,
                SessionState::Starting | SessionState::Running
            ) {
                let _ = session.stop();
            }
            session.cancel.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SessionRequest {
        SessionRequest {
            game_id: 42,
            exe: PathBuf::from("/games/demo.exe"),
            container: PathBuf::from("/data/containers/demo"),
            runtime: PathBuf::from("/data/runtime"),
            memory_limit_mb: 2048,
        }
    }

    #[test]
    fn starts_and_stops() {
        let mut guard = SessionGuard::start(request()).unwrap();
        assert_eq!(guard.state(), SessionState::Running);
        assert_eq!(guard.request().unwrap().game_id, 42);
        guard.stop().unwrap();
        assert_eq!(guard.state(), SessionState::Exited);
        assert!(guard.cancel_token().unwrap().load(Ordering::Acquire));
        assert_eq!(guard.stop(), Err(SessionError::InvalidTransition));
    }

    #[test]
    fn drop_cancels_without_leaking_state() {
        let cancel = {
            let guard = SessionGuard::start(request()).unwrap();
            guard.cancel_token().unwrap()
        };
        assert!(cancel.load(Ordering::Acquire));
    }

    #[test]
    fn rejects_double_start() {
        let mut guard = SessionGuard::start(request()).unwrap();
        guard.stop().unwrap();
        assert_eq!(guard.stop(), Err(SessionError::InvalidTransition));
    }
}
