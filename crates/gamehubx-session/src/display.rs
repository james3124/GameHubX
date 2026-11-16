//! R6 display-bridge ownership contract.
//!
//! The Winlator X11/`ludashix11`/Vulkan renderer stack stays in C/C++ for this
//! phase. This module defines the Rust-side owner that will wrap the native C
//! ABI: one non-cloneable [`DisplayBridge`] per session owns at most one
//! Android surface and one opaque native resource.
//!
//! # Intended C ABI (not implemented yet)
//!
//! Rust owns the Android `Surface`/`SurfaceHolder` lifecycle. The thin Java
//! session host only forwards surface callbacks; it never retains renderer or
//! X11 state. The eventual exports look like:
//!
//! ```c
//! typedef struct ghx_display ghx_display; /* Box<DisplayBridge<FfiBackend>> */
//!
//! ghx_display *ghx_display_create(uint64_t generation);
//! int ghx_display_attach(ghx_display *d, uint64_t generation,
//!                        void *native_window, uint32_t width, uint32_t height);
//! int ghx_display_resize(ghx_display *d, uint64_t generation,
//!                        uint32_t width, uint32_t height);
//! void ghx_display_detach(ghx_display *d, uint64_t generation);
//! void ghx_display_destroy(ghx_display *d);
//! ```
//!
//! Contract rules:
//!
//! - Handles are opaque and non-null: [`NativeHandle`] wraps `NonZeroU64`, so
//!   the token is also usable as a pointer token across the ABI. Each live
//!   handle is paired with a monotonically increasing [`Generation`].
//! - `create` and `destroy` are explicit and idempotent. A second `destroy` is
//!   a no-op; [`Drop`] destroys a live handle exactly once.
//! - Every asynchronous native callback (surface changed, frame presented,
//!   error) carries the generation that created it.
//!   [`DisplayBridge::accept_callback`] rejects stale generations and callbacks
//!   that arrive after detach.
//! - Rust owns create/destroy; the C++ side must never tear down the X11
//!   window, EGL/Vulkan context, or Android surface on its own.
//! - [`DisplayBridge`] is not `Clone`; exactly one owner exists per session.

use core::ffi::c_void;
use core::num::NonZeroU64;

/// Monotonic display generation. A new generation is issued for every
/// successful create after a destroy, so late callbacks from an older native
/// handle can be distinguished and rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(u64);

impl Generation {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Opaque, non-null native display token.
///
/// The token doubles as the pointer-sized opaque value handed across the C ABI;
/// Rust only ever treats it as a token, never dereferences it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeHandle(NonZeroU64);

impl NativeHandle {
    /// Build a handle from a raw non-zero token. Returns `None` for null.
    pub fn from_token(token: u64) -> Option<Self> {
        NonZeroU64::new(token).map(Self)
    }

    pub fn token(self) -> u64 {
        self.0.get()
    }

    /// Pointer form for the eventual C ABI. The value is opaque and must not be
    /// dereferenced by Rust.
    pub fn as_ptr(self) -> *mut c_void {
        self.token() as usize as *mut c_void
    }
}

/// Explicit display states owned by [`DisplayBridge`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayState {
    /// The native display exists but no Android surface is attached.
    Detached,
    /// The native display has a surface attached at this extent.
    Attached { width: u32, height: u32 },
    /// Tear-down is in progress and the native resource has been consumed.
    Destroying,
    /// No native resource is live. A subsequent `create` starts a new generation.
    Destroyed,
}

/// Failure modes of the display bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayError<E> {
    /// The native backend rejected an operation.
    Backend(E),
    /// Re-entrant operation while a destroy is in progress.
    Busy,
    /// Surface extent has a zero dimension.
    InvalidSize,
    /// No live native display exists.
    NotCreated,
    /// A resize was requested while no surface is attached.
    NotAttached,
    /// The caller used a generation that is not the live one (or none is live).
    StaleGeneration {
        expected: Option<Generation>,
        got: Generation,
    },
}

/// Native operations required by [`DisplayBridge`].
///
/// Implementations must treat `destroy` as the single owner-side release of
/// `Resource`; the bridge guarantees it is called at most once per created
/// resource. Operations receive the generation they belong to so the C++ side
/// can tag callbacks.
pub trait DisplayBackend {
    /// Native resource owned by the bridge (X11 display + renderer context).
    type Resource;
    /// Backend-specific error.
    type Error;

    /// Create the native display for `generation`, returning its opaque token
    /// and owned resource.
    fn create(
        &mut self,
        generation: Generation,
    ) -> Result<(NativeHandle, Self::Resource), Self::Error>;

    /// Attach an Android surface to the native display.
    fn attach(
        &mut self,
        resource: &mut Self::Resource,
        generation: Generation,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error>;

    /// Resize the attached surface.
    fn resize(
        &mut self,
        resource: &mut Self::Resource,
        generation: Generation,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error>;

    /// Detach the surface while keeping the native display alive.
    fn detach(
        &mut self,
        resource: &mut Self::Resource,
        generation: Generation,
    ) -> Result<(), Self::Error>;

    /// Consume and release the native resource.
    fn destroy(
        &mut self,
        generation: Generation,
        resource: Self::Resource,
    ) -> Result<(), Self::Error>;
}

/// Single owner of the display backend, one surface, and one native resource.
///
/// Intentionally not `Clone`: only the session owner may attach, resize,
/// detach, or destroy. `Drop` performs the final `destroy` so every path
/// (normal exit, error, rotation, session kill) releases the native handle at
/// most once.
pub struct DisplayBridge<B: DisplayBackend> {
    backend: B,
    generation: Option<Generation>,
    handle: Option<NativeHandle>,
    resource: Option<B::Resource>,
    state: DisplayState,
    next_generation: u64,
}

impl<B: DisplayBackend> DisplayBridge<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            generation: None,
            handle: None,
            resource: None,
            state: DisplayState::Destroyed,
            next_generation: 0,
        }
    }

    pub fn state(&self) -> DisplayState {
        self.state
    }

    pub fn generation(&self) -> Option<Generation> {
        self.generation
    }

    pub fn handle(&self) -> Option<NativeHandle> {
        self.handle
    }

    pub fn is_live(&self) -> bool {
        self.handle.is_some()
    }

    /// Create the native display. Idempotent while a handle is live: the
    /// current generation is returned without touching the backend. After a
    /// destroy this issues a fresh, higher generation.
    pub fn create(&mut self) -> Result<Generation, DisplayError<B::Error>> {
        match self.state {
            DisplayState::Detached | DisplayState::Attached { .. } => {
                return self.generation.ok_or(DisplayError::NotCreated);
            }
            DisplayState::Destroying => return Err(DisplayError::Busy),
            DisplayState::Destroyed => {}
        }

        let next = self
            .next_generation
            .checked_add(1)
            .ok_or(DisplayError::Busy)?;
        let generation = Generation(next);
        let (handle, resource) = self
            .backend
            .create(generation)
            .map_err(DisplayError::Backend)?;
        self.next_generation = next;
        self.handle = Some(handle);
        self.resource = Some(resource);
        self.generation = Some(generation);
        self.state = DisplayState::Detached;
        Ok(generation)
    }

    /// Attach a surface of `width` x `height`. Idempotent for the same extent;
    /// an attached surface with a different extent is treated as a resize.
    pub fn attach(
        &mut self,
        generation: Generation,
        width: u32,
        height: u32,
    ) -> Result<(), DisplayError<B::Error>> {
        self.require_generation(generation)?;
        if width == 0 || height == 0 {
            return Err(DisplayError::InvalidSize);
        }
        match self.state {
            DisplayState::Attached {
                width: current_width,
                height: current_height,
            } if (current_width, current_height) == (width, height) => return Ok(()),
            DisplayState::Attached { .. } => return self.resize(generation, width, height),
            DisplayState::Detached => {}
            DisplayState::Destroying => return Err(DisplayError::Busy),
            DisplayState::Destroyed => return Err(DisplayError::NotCreated),
        }

        let resource = self.resource.as_mut().ok_or(DisplayError::NotCreated)?;
        self.backend
            .attach(resource, generation, width, height)
            .map_err(DisplayError::Backend)?;
        self.state = DisplayState::Attached { width, height };
        Ok(())
    }

    /// Resize the attached surface. A no-op for the current extent, making
    /// repeated rotation callbacks safe.
    pub fn resize(
        &mut self,
        generation: Generation,
        width: u32,
        height: u32,
    ) -> Result<(), DisplayError<B::Error>> {
        self.require_generation(generation)?;
        if width == 0 || height == 0 {
            return Err(DisplayError::InvalidSize);
        }
        let (current_width, current_height) = match self.state {
            DisplayState::Attached { width, height } => (width, height),
            DisplayState::Detached => return Err(DisplayError::NotAttached),
            DisplayState::Destroying => return Err(DisplayError::Busy),
            DisplayState::Destroyed => return Err(DisplayError::NotCreated),
        };
        if (current_width, current_height) == (width, height) {
            return Ok(());
        }

        let resource = self.resource.as_mut().ok_or(DisplayError::NotCreated)?;
        self.backend
            .resize(resource, generation, width, height)
            .map_err(DisplayError::Backend)?;
        self.state = DisplayState::Attached { width, height };
        Ok(())
    }

    /// Detach the surface but keep the native display. Idempotent when already
    /// detached.
    pub fn detach(&mut self, generation: Generation) -> Result<(), DisplayError<B::Error>> {
        self.require_generation(generation)?;
        match self.state {
            DisplayState::Detached | DisplayState::Destroyed => return Ok(()),
            DisplayState::Destroying => return Err(DisplayError::Busy),
            DisplayState::Attached { .. } => {}
        }

        let resource = self.resource.as_mut().ok_or(DisplayError::NotCreated)?;
        self.backend
            .detach(resource, generation)
            .map_err(DisplayError::Backend)?;
        self.state = DisplayState::Detached;
        Ok(())
    }

    /// Destroy the native display and any attached surface. Idempotent: a
    /// second call is a no-op and never reaches the backend again.
    pub fn destroy(&mut self) -> Result<(), DisplayError<B::Error>> {
        if matches!(self.state, DisplayState::Destroyed) {
            return Ok(());
        }

        let generation = self.generation.take();
        let resource = self.resource.take();
        self.handle = None;
        self.state = DisplayState::Destroying;

        let result = match (generation, resource) {
            (Some(generation), Some(resource)) => self.backend.destroy(generation, resource),
            _ => Ok(()),
        };
        self.state = DisplayState::Destroyed;
        result.map_err(DisplayError::Backend)
    }

    /// Validate an asynchronous native callback against the live generation.
    ///
    /// Accepted only while a surface is attached and the generation matches.
    /// Callbacks from a destroyed display, from an older generation, or after
    /// detach are rejected instead of touching native state.
    pub fn accept_callback(&self, generation: Generation) -> Result<(), DisplayError<B::Error>> {
        if matches!(self.state, DisplayState::Attached { .. })
            && self.generation == Some(generation)
        {
            Ok(())
        } else {
            Err(DisplayError::StaleGeneration {
                expected: self.generation,
                got: generation,
            })
        }
    }

    fn require_generation(&self, generation: Generation) -> Result<(), DisplayError<B::Error>> {
        match self.generation {
            Some(current) if current == generation => Ok(()),
            expected => Err(DisplayError::StaleGeneration {
                expected,
                got: generation,
            }),
        }
    }
}

impl<B: DisplayBackend> Drop for DisplayBridge<B> {
    fn drop(&mut self) {
        // `destroy` is idempotent and marks the bridge destroyed even if the
        // backend reports an error, so this runs the native destructor exactly
        // once for a live handle and never twice.
        let _ = self.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Debug, Default)]
    struct MockStats {
        creates: AtomicUsize,
        attaches: AtomicUsize,
        resizes: AtomicUsize,
        detaches: AtomicUsize,
        destroys: AtomicUsize,
        live: AtomicUsize,
    }

    impl MockStats {
        fn creates(&self) -> usize {
            self.creates.load(Ordering::SeqCst)
        }

        fn attaches(&self) -> usize {
            self.attaches.load(Ordering::SeqCst)
        }

        fn resizes(&self) -> usize {
            self.resizes.load(Ordering::SeqCst)
        }

        fn detaches(&self) -> usize {
            self.detaches.load(Ordering::SeqCst)
        }

        fn destroys(&self) -> usize {
            self.destroys.load(Ordering::SeqCst)
        }

        fn live(&self) -> usize {
            self.live.load(Ordering::SeqCst)
        }
    }

    #[derive(Debug)]
    struct MockBackend {
        stats: Arc<MockStats>,
        next_token: u64,
    }

    impl MockBackend {
        fn new() -> (Self, Arc<MockStats>) {
            let stats = Arc::new(MockStats::default());
            (
                Self {
                    stats: Arc::clone(&stats),
                    next_token: 0,
                },
                stats,
            )
        }
    }

    impl DisplayBackend for MockBackend {
        type Resource = ();
        type Error = Infallible;

        fn create(
            &mut self,
            _generation: Generation,
        ) -> Result<(NativeHandle, Self::Resource), Self::Error> {
            self.next_token += 1;
            let handle = NativeHandle::from_token(self.next_token).expect("mock token is non-zero");
            self.stats.creates.fetch_add(1, Ordering::SeqCst);
            self.stats.live.fetch_add(1, Ordering::SeqCst);
            Ok((handle, ()))
        }

        fn attach(
            &mut self,
            _resource: &mut Self::Resource,
            _generation: Generation,
            _width: u32,
            _height: u32,
        ) -> Result<(), Self::Error> {
            self.stats.attaches.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn resize(
            &mut self,
            _resource: &mut Self::Resource,
            _generation: Generation,
            _width: u32,
            _height: u32,
        ) -> Result<(), Self::Error> {
            self.stats.resizes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn detach(
            &mut self,
            _resource: &mut Self::Resource,
            _generation: Generation,
        ) -> Result<(), Self::Error> {
            self.stats.detaches.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn destroy(
            &mut self,
            _generation: Generation,
            _resource: Self::Resource,
        ) -> Result<(), Self::Error> {
            self.stats.destroys.fetch_add(1, Ordering::SeqCst);
            self.stats.live.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn mock_bridge() -> (DisplayBridge<MockBackend>, Arc<MockStats>) {
        let (backend, stats) = MockBackend::new();
        (DisplayBridge::new(backend), stats)
    }

    #[test]
    fn fifty_create_attach_resize_detach_destroy_cycles_balance() {
        let (mut bridge, stats) = mock_bridge();
        let mut previous = None;

        for _ in 0..50 {
            let generation = bridge.create().unwrap();
            if let Some(previous) = previous {
                assert!(generation > previous);
            }
            previous = Some(generation);
            assert_eq!(bridge.state(), DisplayState::Detached);
            assert!(bridge.is_live());

            bridge.attach(generation, 1280, 720).unwrap();
            assert_eq!(
                bridge.state(),
                DisplayState::Attached {
                    width: 1280,
                    height: 720
                }
            );
            bridge.resize(generation, 1920, 1080).unwrap();
            assert_eq!(
                bridge.state(),
                DisplayState::Attached {
                    width: 1920,
                    height: 1080
                }
            );
            bridge.detach(generation).unwrap();
            assert_eq!(bridge.state(), DisplayState::Detached);
            bridge.destroy().unwrap();
            assert_eq!(bridge.state(), DisplayState::Destroyed);
            assert!(!bridge.is_live());
        }

        assert_eq!(stats.creates(), 50);
        assert_eq!(stats.attaches(), 50);
        assert_eq!(stats.resizes(), 50);
        assert_eq!(stats.detaches(), 50);
        assert_eq!(stats.destroys(), 50);
        assert_eq!(stats.live(), 0);

        drop(bridge);
        assert_eq!(stats.destroys(), 50);
        assert_eq!(stats.live(), 0);
    }

    #[test]
    fn stale_generations_and_post_detach_callbacks_are_rejected() {
        let (mut bridge, _stats) = mock_bridge();
        let first = bridge.create().unwrap();

        // No surface yet: even the live generation may not drive callbacks.
        assert_eq!(
            bridge.accept_callback(first),
            Err(DisplayError::StaleGeneration {
                expected: Some(first),
                got: first
            })
        );

        bridge.attach(first, 800, 600).unwrap();
        assert_eq!(bridge.accept_callback(first), Ok(()));
        assert_eq!(bridge.create().unwrap(), first);
        bridge.resize(first, 1024, 768).unwrap();

        // Detach invalidates callbacks for the still-live generation.
        bridge.detach(first).unwrap();
        assert_eq!(
            bridge.accept_callback(first),
            Err(DisplayError::StaleGeneration {
                expected: Some(first),
                got: first
            })
        );

        bridge.destroy().unwrap();
        let second = bridge.create().unwrap();
        assert!(second > first);

        let stale = DisplayError::StaleGeneration {
            expected: Some(second),
            got: first,
        };
        assert_eq!(bridge.attach(first, 800, 600), Err(stale));
        assert_eq!(bridge.resize(first, 800, 600), Err(stale));
        assert_eq!(bridge.detach(first), Err(stale));
        assert_eq!(bridge.accept_callback(first), Err(stale));

        bridge.attach(second, 800, 600).unwrap();
        assert_eq!(bridge.accept_callback(first), Err(stale));
        assert_eq!(bridge.accept_callback(second), Ok(()));
    }

    #[test]
    fn double_destroy_is_consumed_exactly_once() {
        let (mut bridge, stats) = mock_bridge();

        // Destroy before create is a harmless no-op.
        assert_eq!(bridge.destroy(), Ok(()));
        assert_eq!(stats.destroys(), 0);

        let generation = bridge.create().unwrap();
        bridge.attach(generation, 640, 480).unwrap();
        bridge.destroy().unwrap();
        bridge.destroy().unwrap();
        assert_eq!(stats.destroys(), 1);
        assert_eq!(stats.live(), 0);

        // Destroy is terminal for this handle but the bridge may create again.
        bridge.create().unwrap();
        drop(bridge);
        assert_eq!(stats.creates(), 2);
        assert_eq!(stats.destroys(), 2);
        assert_eq!(stats.live(), 0);
    }

    #[test]
    fn drop_destroys_live_handle_exactly_once() {
        let (bridge, stats) = mock_bridge();
        drop(bridge);
        assert_eq!(stats.creates(), 0);
        assert_eq!(stats.destroys(), 0);

        let (mut bridge, stats) = mock_bridge();
        let generation = bridge.create().unwrap();
        bridge.attach(generation, 1024, 768).unwrap();
        assert_eq!(bridge.accept_callback(generation), Ok(()));
        assert_eq!(stats.live(), 1);

        drop(bridge);
        assert_eq!(stats.creates(), 1);
        assert_eq!(stats.destroys(), 1);
        assert_eq!(stats.live(), 0);
    }

    #[test]
    fn repeated_lifecycle_calls_are_idempotent() {
        let (mut bridge, stats) = mock_bridge();
        let generation = bridge.create().unwrap();
        assert_eq!(bridge.create().unwrap(), generation);
        assert_eq!(stats.creates(), 1);

        bridge.attach(generation, 800, 600).unwrap();
        bridge.attach(generation, 800, 600).unwrap();
        assert_eq!(stats.attaches(), 1);

        // Same extent is a no-op, different extent is one resize.
        bridge.resize(generation, 800, 600).unwrap();
        assert_eq!(stats.resizes(), 0);
        bridge.resize(generation, 1024, 768).unwrap();
        bridge.resize(generation, 1024, 768).unwrap();
        assert_eq!(stats.resizes(), 1);

        bridge.detach(generation).unwrap();
        bridge.detach(generation).unwrap();
        assert_eq!(stats.detaches(), 1);

        bridge.destroy().unwrap();
        bridge.destroy().unwrap();
        assert_eq!(stats.destroys(), 1);
    }

    #[test]
    fn invalid_and_out_of_order_operations_are_rejected() {
        let (mut bridge, stats) = mock_bridge();
        let generation = bridge.create().unwrap();

        assert_eq!(
            bridge.attach(generation, 0, 600),
            Err(DisplayError::InvalidSize)
        );
        assert_eq!(
            bridge.resize(generation, 800, 600),
            Err(DisplayError::NotAttached)
        );
        assert_eq!(stats.attaches(), 0);
        assert_eq!(stats.resizes(), 0);

        bridge.attach(generation, 800, 600).unwrap();
        assert_eq!(
            bridge.resize(generation, 800, 0),
            Err(DisplayError::InvalidSize)
        );
        assert_eq!(stats.resizes(), 0);
    }
}
