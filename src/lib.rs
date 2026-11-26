//! The C ABI between nexus-cli (the host) and its plugins.
//!
//! **Draft for review.** Types and signatures only. The policy behind them is
//! in "Runtime Plugin Interface.md".
//!
//! # Loading
//!
//! A plugin library exports two symbols, `nexus_get_version`
//! ([`GetVersion`]) and `nexus_get_capabilities` ([`GetCapabilities`]).
//! Everything else is reached through the [`PluginVTable`] of each plugin the
//! library reports. Libraries stay loaded until the process exits.
//!
//! # Call order
//!
//! ```text
//! library    get_version, get_capabilities
//! plugin     get_config
//! instance   init, set_config, get_series (data sources), start,
//!            { poll | touch | render }, shutdown
//! ```
//!
//! `shutdown` is called exactly once for every successful `init`, including
//! when a later call failed or `start` was never reached. The exception: at
//! exit the host waits only 2 seconds for instances to shut down, so one still
//! in a call may never receive it.
//!
//! # Threads
//!
//! All calls for one instance, from `init` to `shutdown`, come from one host
//! thread that belongs to that instance. Different instances run
//! concurrently, so anything they share must be thread-safe. Calls may block.
//!
//! # Memory
//!
//! Neither side frees memory that the other allocated.
//!
//! | Data | Valid until |
//! |---|---|
//! | Arguments passed to a plugin | the call returns |
//! | [`Host`] | `shutdown` returns |
//! | Returned by an instance call | the next call to that instance |
//! | Returned by a library or plugin call | the process exits |
//!
//! The host copies series and pixels on the instance's thread as soon as the
//! call that returned them completes.
//!
//! # Values
//!
//! Strings are UTF-8 as pointer and length, not NUL-terminated. Values that
//! cross the boundary are integer newtypes and `u8` flags, never Rust enums
//! or `bool`, so an unexpected value is not undefined behaviour.
//!
//! # Panics
//!
//! A panic that reaches an `extern "C"` function aborts the whole process.
//! Plugins must catch panics and return [`Status::PANIC`]. Functions that
//! return no [`Status`] must not panic.
//!
//! # Compatibility
//!
//! A change in A or B of the interface version breaks compatibility. A change
//! in C may only append fields to structs passed by pointer: [`Host`],
//! [`PluginVTable`], [`InstanceConfig`] and [`RenderInput`]. Check the other
//! side's version before reading an appended field.

#![no_std]

use core::ffi::c_void;

/// `[0, A, B, C]`.
pub type Version = [u8; 4];

/// The version of this interface.
pub const INTERFACE_VERSION: Version = [0, 0, 1, 0];

/// Width of the display, and of the widest section.
pub const MAX_WIDTH: u32 = 640;
/// Height of the display, and of every section.
pub const MAX_HEIGHT: u32 = 48;

// ----------------------------------------------------------------- basic types

/// Borrowed UTF-8 text. When `len` is 0, `ptr` may be null.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Str {
    pub ptr: *const u8,
    pub len: usize,
}

// SAFETY: a `Str` is a read-only view; lets plugins keep descriptors in statics.
unsafe impl Sync for Str {}

/// Borrowed array. When `len` is 0, `ptr` may be null.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Slice<T> {
    pub ptr: *const T,
    pub len: usize,
}

/// Result of a call.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status(pub u32);

impl Status {
    pub const OK: Status = Status(0);
    /// This call failed. After a failure from `set_config`, `get_series` or
    /// `start` the host discards the instance. After any other call the
    /// instance is still usable, though the host may discard it. Discarding
    /// means calling `shutdown`; the application keeps running.
    pub const ERROR: Status = Status(1);
    /// The plugin caught a panic. The only further call is `shutdown`.
    pub const PANIC: Status = Status(2);
}

/// Host-owned space for a readable error message. When returning anything
/// other than [`Status::OK`], a plugin may write up to `capacity` bytes of
/// UTF-8 to `data` and set `len`.
#[repr(C)]
#[derive(Debug)]
pub struct ErrorBuffer {
    pub data: *mut u8,
    pub capacity: usize,
    pub len: usize,
}

// --------------------------------------------------------------------- library

pub const GET_VERSION_SYMBOL: &str = "nexus_get_version";
pub const GET_CAPABILITIES_SYMBOL: &str = "nexus_get_capabilities";

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Versions {
    /// The library's own version. Informational.
    pub plugin: Version,
    /// The [`INTERFACE_VERSION`] the library was built against.
    pub interface: Version,
}

/// Called before anything else. This signature never changes.
pub type GetVersion = unsafe extern "C" fn() -> Versions;

/// The plugins in this library.
pub type GetCapabilities = unsafe extern "C" fn() -> Slice<PluginDescriptor>;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PluginKind(pub u32);

impl PluginKind {
    /// Interactive.
    pub const BUTTON: PluginKind = PluginKind(1);
    /// Interactive.
    pub const SCROLL_BAR: PluginKind = PluginKind(2);
    /// Takes no screen space.
    pub const DATA_SOURCE: PluginKind = PluginKind(3);
    /// Draws data from sources. Passive, and may be stacked.
    pub const VISUAL: PluginKind = PluginKind(4);
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PluginDescriptor {
    /// What `plugin:` names in the app config. If two plugins share a name,
    /// the first one loaded is used and later ones are rejected.
    pub name: Str,
    pub kind: PluginKind,
    pub vtable: *const PluginVTable,
}

// SAFETY: points only to immutable data that lives as long as the library.
unsafe impl Sync for PluginDescriptor {}

// ---------------------------------------------------------------------- plugin

/// A plugin's functions. Required for every kind: `get_config`, `init`,
/// `set_config`, `start` and `shutdown`. Others are noted per field; leave
/// those that don't apply as `None`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PluginVTable {
    pub get_config: Option<GetConfig>,
    pub init: Option<Init>,
    pub set_config: Option<SetConfig>,
    /// Data sources.
    pub get_series: Option<GetSeries>,
    pub start: Option<Start>,
    pub shutdown: Option<Shutdown>,
    /// Data sources.
    pub poll: Option<Poll>,
    /// Buttons and scroll bars.
    pub touch: Option<Touch>,
    /// Buttons, scroll bars and visuals.
    pub render: Option<Render>,
}

/// An instance's state. Only the plugin knows what is behind the pointer.
#[repr(C)]
pub struct Instance {
    _opaque: [u8; 0],
}

/// The default `config` block for the named plugin, as YAML.
pub type GetConfig = unsafe extern "C" fn(name: Str) -> Str;

/// Create an instance of the named plugin and store it in `instance`. On
/// failure nothing is created and `shutdown` is not called.
pub type Init = unsafe extern "C" fn(
    name: Str,
    host: *const Host,
    instance: *mut *mut Instance,
    error: *mut ErrorBuffer,
) -> Status;

/// Validate and apply the config. Called once. Unknown keys, and sizes or
/// series the plugin cannot draw, are errors.
pub type SetConfig = unsafe extern "C" fn(
    instance: *mut Instance,
    config: *const InstanceConfig,
    error: *mut ErrorBuffer,
) -> Status;

/// The series this source supplies. The list is fixed from here on.
pub type GetSeries = unsafe extern "C" fn(
    instance: *mut Instance,
    series: *mut Slice<SeriesInfo>,
    error: *mut ErrorBuffer,
) -> Status;

/// Begin work.
pub type Start = unsafe extern "C" fn(instance: *mut Instance, error: *mut ErrorBuffer) -> Status;

/// Stop every thread the instance started, then free it. After this returns
/// the plugin must not use [`Host`].
pub type Shutdown = unsafe extern "C" fn(instance: *mut Instance);

/// The full active data set: the same series as [`GetSeries`] reported, in
/// the same order.
pub type Poll = unsafe extern "C" fn(
    instance: *mut Instance,
    series: *mut Slice<Series>,
    error: *mut ErrorBuffer,
) -> Status;

/// One touch event for a gesture this instance owns. While the instance is
/// busy the host keeps only the latest [`TouchPhase::MOVE`]; every other
/// phase is delivered, in order.
pub type Touch = unsafe extern "C" fn(
    instance: *mut Instance,
    event: TouchEvent,
    error: *mut ErrorBuffer,
) -> Status;

/// Draw the instance. The host zeroes `output` before the call.
pub type Render = unsafe extern "C" fn(
    instance: *mut Instance,
    input: *const RenderInput,
    output: *mut RenderOutput,
    error: *mut ErrorBuffer,
) -> Status;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InstanceConfig {
    /// The instance's `config` block as YAML. Empty if there is none.
    pub yaml: Str,
    /// The app config file's folder, for resolving relative paths.
    pub base_dir: Str,
    /// Size of the instance's area. Both 0 for a data source.
    pub width: u32,
    pub height: u32,
    /// Time between polls, 100..=3_600_000. 0 unless a data source.
    pub update_interval_ms: u32,
    /// Visuals: the series selected by `source`, in the order named there.
    /// Empty otherwise.
    pub series: Slice<SeriesInfo>,
}

// ------------------------------------------------------------------------ data

/// Which field of [`Value`], and what type of array, a series uses.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElementType(pub u32);

impl ElementType {
    pub const U32: ElementType = ElementType(1);
    pub const U64: ElementType = ElementType(2);
    pub const I32: ElementType = ElementType(3);
    pub const I64: ElementType = ElementType(4);
    pub const F32: ElementType = ElementType(5);
    pub const F64: ElementType = ElementType(6);
}

/// One value of a series' element type.
#[repr(C)]
#[derive(Clone, Copy)]
pub union Value {
    pub u32: u32,
    pub u64: u64,
    pub i32: i32,
    pub i64: i64,
    pub f32: f32,
    pub f64: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SeriesInfo {
    /// `total` in `cpu.total`. Unique within its source.
    pub name: Str,
    /// For people.
    pub label: Str,
    /// May be empty.
    pub unit: Str,
    pub element_type: ElementType,
    /// Non-zero if `min` is set.
    pub has_min: u8,
    /// Non-zero if `max` is set.
    pub has_max: u8,
    pub min: Value,
    pub max: Value,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Series {
    pub info: SeriesInfo,
    /// `len` values of `info.element_type`, aligned for that type, oldest
    /// first. Null is allowed when `len` is 0.
    pub values: *const c_void,
    pub len: usize,
}

// ----------------------------------------------------------------------- touch

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchPhase(pub u32);

impl TouchPhase {
    pub const DOWN: TouchPhase = TouchPhase(1);
    pub const MOVE: TouchPhase = TouchPhase(2);
    pub const UP: TouchPhase = TouchPhase(3);
    /// The gesture ended without a detected lift. Activate nothing.
    pub const LOST: TouchPhase = TouchPhase(4);
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TouchEvent {
    /// Pixels from the section's left edge; may fall outside the section. For
    /// [`TouchPhase::LOST`], the last position seen.
    pub x: i32,
    pub phase: TouchPhase,
}

// ------------------------------------------------------------------- rendering

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenderInput {
    /// Visuals: the latest data, matching [`InstanceConfig::series`] in
    /// order. A series not yet polled has `len` 0. Empty for other kinds.
    pub series: Slice<Series>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RenderOutput {
    /// Premultiplied BGRA, `width` x `height` from [`InstanceConfig`], rows
    /// top to bottom with no padding.
    pub pixels: *const u8,
    /// Bytes at `pixels`; must equal `width * height * 4`.
    pub len: usize,
    /// Milliseconds until `render` should be called again; 0 for no timed
    /// call.
    pub next_render_ms: u32,
}

// --------------------------------------------------------------- host services

/// Identifies the instance to the host. Pass it back unchanged.
#[repr(C)]
pub struct HostContext {
    _opaque: [u8; 0],
}

/// Services for one instance. Every function may be called from any of the
/// plugin's threads.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Host {
    /// The host's [`INTERFACE_VERSION`].
    pub interface_version: Version,
    pub context: *mut HostContext,
    /// Ask for `render` to be called. Does not call it directly.
    pub request_render: unsafe extern "C" fn(context: *mut HostContext),
    /// Draw one line of text centred in `request.rect`, ending in an ellipsis
    /// if too long, over what `canvas` already holds. Only pixels inside both
    /// the rect and the canvas change.
    pub draw_text: unsafe extern "C" fn(
        context: *mut HostContext,
        request: *const TextRequest,
        canvas: *mut Canvas,
        error: *mut ErrorBuffer,
    ) -> Status,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TextRequest {
    pub text: Str,
    /// Font family; empty for the system UI font.
    pub font: Str,
    /// Font height in pixels.
    pub px_height: u32,
    pub color: Rgb,
    pub rect: Rect,
}

/// A plugin-owned premultiplied BGRA buffer for the host to draw into, rows
/// top to bottom with no padding. `len` must equal `width * height * 4`.
#[repr(C)]
#[derive(Debug)]
pub struct Canvas {
    pub pixels: *mut u8,
    pub len: usize,
    pub width: u32,
    pub height: u32,
}
