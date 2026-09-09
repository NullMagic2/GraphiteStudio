//! Optional Wacom / XP-Pen / Gaomon Wintab input. ABI and axis conventions follow Wacom's Wintab
//! reference and official WintabDN / TiltTest examples. No driver DLL is bundled.
use crate::input::{PenPhase, PenSample};
use std::{
    ffi::c_void,
    ptr::null_mut,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE, HWND, POINT},
    Graphics::Gdi::ScreenToClient,
    System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32},
};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

const STATUS: u32 = 2;
const TIME: u32 = 4;
const CURSOR: u32 = 0x20;
const BUTTONS: u32 = 0x40;
const X: u32 = 0x80;
const Y: u32 = 0x100;
const PRESSURE: u32 = 0x400;
const ORIENTATION: u32 = 0x1000;
const BASE: u32 = STATUS | TIME | CURSOR | BUTTONS | X | Y;
const ALLOWED: u32 = BASE | PRESSURE | ORIENTATION;

#[repr(C)]
struct LogContext {
    name: [u8; 40],
    options: u32,
    status: u32,
    locks: u32,
    msg_base: u32,
    device: u32,
    packet_rate: u32,
    packet_data: u32,
    packet_mode: u32,
    move_mask: u32,
    button_down: u32,
    button_up: u32,
    in_origin: [i32; 3],
    in_extent: [i32; 3],
    out_origin: [i32; 3],
    out_extent: [i32; 3],
    sensitivity: [u32; 3],
    system_mode: i32,
    system_origin: [i32; 2],
    system_extent: [i32; 2],
    system_sensitivity: [u32; 2],
}
impl Default for LogContext {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Axis {
    min: i32,
    max: i32,
    units: u32,
    resolution: u32,
}
impl Axis {
    fn pressure(self, value: u32) -> Option<f32> {
        (self.max > self.min).then(|| {
            ((value as f64 - self.min as f64) / (self.max as f64 - self.min as f64)).clamp(0., 1.)
                as f32
        })
    }
    fn degrees(self, value: i32) -> Option<f32> {
        (self.resolution > 0 && self.max > self.min)
            .then(|| value as f32 * 360. / (self.resolution as f32 / 65536.))
    }
}

type Info = unsafe extern "system" fn(u32, u32, *mut c_void) -> u32;
type Open = unsafe extern "system" fn(HWND, *mut LogContext, i32) -> *mut c_void;
type Close = unsafe extern "system" fn(*mut c_void) -> i32;
type Enable = unsafe extern "system" fn(*mut c_void, i32) -> i32;
type Get = unsafe extern "system" fn(*mut c_void, i32, *mut c_void) -> i32;
struct Api {
    module: HMODULE,
    info: Info,
    open: Open,
    close: Close,
    enable: Enable,
    overlap: Enable,
    get: Get,
}
impl Api {
    fn load() -> Option<Self> {
        let name: Vec<u16> = "Wintab32.dll\0".encode_utf16().collect();
        // Only load the installed driver from the Windows system directory.
        let module =
            unsafe { LoadLibraryExW(name.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
        if module.is_null() {
            return None;
        }
        macro_rules! symbol {
            ($name:literal,$ty:ty) => {{
                let Some(proc) = (unsafe { GetProcAddress(module, concat!($name, "\0").as_ptr()) })
                else {
                    unsafe { FreeLibrary(module) };
                    return None;
                };
                unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(proc) }
            }};
        }
        Some(Self {
            module,
            info: symbol!("WTInfoA", Info),
            open: symbol!("WTOpenA", Open),
            close: symbol!("WTClose", Close),
            enable: symbol!("WTEnable", Enable),
            overlap: symbol!("WTOverlap", Enable),
            get: symbol!("WTPacketsGet", Get),
        })
    }
    fn query<T>(&self, category: u32, index: u32, out: &mut T) -> bool {
        let size = unsafe { (self.info)(category, index, null_mut()) } as usize;
        if size == 0 || size > std::mem::size_of::<T>() {
            return false;
        }
        unsafe { (self.info)(category, index, out as *mut T as *mut c_void) != 0 }
    }
}
impl Drop for Api {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.module);
        }
    }
}

#[derive(Default, Debug)]
struct Packet {
    status: u32,
    time: u32,
    cursor: u32,
    buttons: u32,
    x: i32,
    y: i32,
    pressure: Option<u32>,
    orientation: Option<[i32; 3]>,
}
fn stride(mask: u32) -> Option<usize> {
    if mask & !ALLOWED != 0 || mask & (X | Y | BUTTONS) != (X | Y | BUTTONS) {
        return None;
    }
    Some(mask.count_ones() as usize + if mask & ORIENTATION != 0 { 2 } else { 0 })
}
fn decode(words: &[u32], mask: u32) -> Option<Packet> {
    let count = stride(mask)?;
    if words.len() < count {
        return None;
    }
    let mut packet = Packet::default();
    let mut n = 0;
    for bit in [STATUS, TIME, CURSOR, BUTTONS, X, Y, PRESSURE, ORIENTATION] {
        if mask & bit == 0 {
            continue;
        }
        let value = words[n];
        n += 1;
        match bit {
            STATUS => packet.status = value,
            TIME => packet.time = value,
            CURSOR => packet.cursor = value,
            BUTTONS => packet.buttons = value,
            X => packet.x = value as i32,
            Y => packet.y = value as i32,
            PRESSURE => packet.pressure = Some(value),
            ORIENTATION => {
                packet.orientation = Some([value as i32, words[n] as i32, words[n + 1] as i32]);
                n += 2;
            }
            _ => unreachable!(),
        }
    }
    Some(packet)
}

fn plane_tilt(axes: [Axis; 3], raw: [i32; 3]) -> Option<[f32; 2]> {
    let azimuth = axes[0].degrees(raw[0])?.to_radians();
    let altitude = axes[1].degrees(raw[1])?.abs().clamp(0.1, 90.);
    let lean = (90. - altitude).to_radians().tan();
    Some([
        (lean * azimuth.sin()).atan().to_degrees(),
        (-lean * azimuth.cos()).atan().to_degrees(),
    ])
}

struct Context {
    api: Api,
    handle: *mut c_void,
    hwnd: HWND,
    mask: u32,
    device: u32,
    pressure: Axis,
    axes: [Axis; 3],
    focused: bool,
    down: bool,
    cursor: Option<u32>,
    tip_bit: Option<u32>,
    cursor_mask: u32,
    last_twist: Option<i32>,
    twist_seen: bool,
}
impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            (self.api.close)(self.handle);
        }
    }
}
impl Context {
    fn open(hwnd: HWND) -> Option<Self> {
        let api = Api::load()?;
        let mut lc = LogContext::default();
        if !api.query(4, 0, &mut lc) {
            return None;
        } // current default system mapping
        let mut pressure = Axis::default();
        let mut axes = [Axis::default(); 3];
        api.query(100 + lc.device, 15, &mut pressure);
        api.query(100 + lc.device, 17, &mut axes);
        lc.name = [0; 40];
        lc.name[..14].copy_from_slice(b"GraphiteStudio");
        lc.options = (lc.options | 1) & !0x0c; // system cursor, polled packets, no WT_PACKET flood
        lc.packet_data =
            BASE | if pressure.max > pressure.min {
                PRESSURE
            } else {
                0
            } | if axes.iter().any(|a| a.resolution > 0) {
                ORIENTATION
            } else {
                0
            };
        lc.packet_mode = 0;
        lc.move_mask = lc.packet_data;
        lc.button_up = lc.button_down;
        // Preserve the driver's selected screen / tablet region; flip only output Y to
        // screen coordinates as in Wacom TiltTest. Never change global driver settings.
        lc.out_origin[0] = lc.system_origin[0];
        lc.out_origin[1] = lc.system_origin[1];
        lc.out_extent[0] = lc.system_extent[0];
        lc.out_extent[1] = -lc.system_extent[1];
        if lc.out_extent[0] == 0 || lc.out_extent[1] == 0 || lc.system_mode != 0 {
            return None;
        }
        let handle = unsafe { (api.open)(hwnd, &mut lc, 0) };
        if handle.is_null() {
            return None;
        }
        if stride(lc.packet_data).is_none() || lc.packet_mode != 0 {
            unsafe { (api.close)(handle) };
            return None;
        }
        if let Some(proc) =
            unsafe { GetProcAddress(api.module, c"WTQueueSizeSet".as_ptr() as *const u8) }
        {
            let resize: Enable = unsafe { std::mem::transmute(proc) };
            unsafe {
                resize(handle, 512);
            }
        }
        Some(Self {
            api,
            handle,
            hwnd,
            mask: lc.packet_data,
            device: lc.device,
            pressure,
            axes,
            focused: false,
            down: false,
            cursor: None,
            tip_bit: None,
            cursor_mask: ALLOWED,
            last_twist: None,
            twist_seen: false,
        })
    }
    fn poll(&mut self, focused: bool) -> Vec<PenSample> {
        let mut samples = Vec::new();
        if focused != self.focused {
            unsafe {
                (self.api.enable)(self.handle, focused as i32);
                if focused {
                    (self.api.overlap)(self.handle, 1);
                }
            }
            self.focused = focused;
            self.down = false;
            samples.push(PenSample {
                phase: PenPhase::Cancel,
                ..Default::default()
            });
        }
        if !focused {
            return samples;
        }
        let size = stride(self.mask).unwrap();
        let mut buffer = vec![0u32; size * 256];
        // Drain a bounded backlog in chronological order rather than keeping only the last point.
        for _ in 0..4 {
            let count =
                unsafe { (self.api.get)(self.handle, 256, buffer.as_mut_ptr() as *mut c_void) }
                    .clamp(0, 256) as usize;
            for words in buffer[..count * size].chunks_exact(size) {
                let Some(p) = decode(words, self.mask) else {
                    continue;
                };
                if p.status & 3 != 0 {
                    // proximity loss or queue overflow: don't bridge lost samples
                    self.down = false;
                    samples.push(PenSample {
                        phase: PenPhase::Cancel,
                        ..Default::default()
                    });
                    continue;
                }
                if self.cursor != Some(p.cursor) {
                    if self.down {
                        samples.push(PenSample {
                            phase: PenPhase::Cancel,
                            ..Default::default()
                        });
                    }
                    self.down = false;
                    self.cursor = Some(p.cursor);
                    self.last_twist = None;
                    self.twist_seen = false;
                    self.cursor_mask = self.mask;
                    self.api.query(200 + p.cursor, 3, &mut self.cursor_mask);
                    self.api.query(100 + self.device, 15, &mut self.pressure);
                    self.api.query(100 + self.device, 17, &mut self.axes);
                    let mut physical = 0u8;
                    let mut mapping = [0u8; 32];
                    self.tip_bit = if self.api.query(200 + p.cursor, 9, &mut physical)
                        && self.api.query(200 + p.cursor, 7, &mut mapping)
                        && (physical as usize) < mapping.len()
                        && mapping[physical as usize] < 32
                    {
                        Some(1u32 << mapping[physical as usize])
                    } else {
                        None
                    };
                }
                let pressure = p
                    .pressure
                    .filter(|_| self.cursor_mask & PRESSURE != 0)
                    .and_then(|p| self.pressure.pressure(p));
                let down = self
                    .tip_bit
                    .map(|bit| p.buttons & bit != 0)
                    .unwrap_or_else(|| pressure.is_some_and(|p| p > 0.));
                let phase = match (self.down, down) {
                    (false, true) => PenPhase::Down,
                    (true, true) => PenPhase::Move,
                    (true, false) => PenPhase::Up,
                    _ => PenPhase::Hover,
                };
                self.down = down;
                let orientation = p
                    .orientation
                    .filter(|_| self.cursor_mask & ORIENTATION != 0);
                let mut rotation_deg = None;
                if let Some(raw) = orientation {
                    self.twist_seen |= self.last_twist.is_some_and(|last| last != raw[2]);
                    self.last_twist = Some(raw[2]);
                    if self.twist_seen {
                        rotation_deg = self.axes[2].degrees(raw[2]);
                    }
                }
                let mut point = POINT { x: p.x, y: p.y };
                if unsafe { ScreenToClient(self.hwnd, &mut point) } == 0 {
                    continue;
                }
                samples.push(PenSample {
                    id: 1,
                    phase,
                    time_ms: p.time,
                    client_px: [point.x as f32, point.y as f32],
                    pressure,
                    tilt_xy: orientation.and_then(|o| plane_tilt(self.axes, o)),
                    rotation_deg,
                });
            }
            if count < 256 {
                break;
            }
        }
        samples
    }
}

#[derive(Default)]
pub struct WintabBridge {
    context: Option<Context>,
    last_attempt: Option<Instant>,
    last_display_check: Option<Instant>,
    displays: Vec<(i32, i32, u32, u32)>,
    pub status: String,
}
impl WintabBridge {
    pub fn update(&mut self, window: &Window, enabled: bool, focused: bool) -> bool {
        if !enabled {
            if self.context.take().is_some() {
                crate::native_pen::push(PenSample {
                    phase: PenPhase::Cancel,
                    ..Default::default()
                });
            }
            self.last_attempt = None;
            self.status.clear();
            crate::native_pen::use_wintab(false);
            return false;
        }
        // Reopen with the driver's current mapping after EasyCanvas or a monitor
        // appears/disappears. Do not keep a stroke alive across a mapping change.
        if self
            .last_display_check
            .is_none_or(|t| t.elapsed() > Duration::from_secs(1))
        {
            self.last_display_check = Some(Instant::now());
            let mut displays: Vec<_> = window
                .available_monitors()
                .map(|m| {
                    let p = m.position();
                    let s = m.size();
                    (p.x, p.y, s.width, s.height)
                })
                .collect();
            displays.sort_unstable();
            if displays != self.displays {
                self.displays = displays;
                if self.context.take().is_some() {
                    crate::native_pen::push(PenSample {
                        phase: PenPhase::Cancel,
                        ..Default::default()
                    });
                }
                self.last_attempt = None;
            }
        }
        if self.context.is_none()
            && self
                .last_attempt
                .is_none_or(|t| t.elapsed() > Duration::from_secs(3))
        {
            self.last_attempt = Some(Instant::now());
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                    self.context = Context::open(handle.hwnd.get() as HWND);
                }
            }
        }
        crate::native_pen::use_wintab(self.context.is_some());
        if let Some(context) = &mut self.context {
            self.status = "Wintab connected · pressure and available pen sensors".into();
            for packet in context.poll(focused) {
                crate::native_pen::push(packet);
            }
            focused
        } else {
            self.status="Wintab unavailable. Windows Ink / mouse fallback remains available. Check the installed tablet driver and use Pen mode.".into();
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gaomon_pressure_ranges_are_driver_reported_not_brand_fixed() {
        for max in [8191,16383] {
            let axis=Axis {min:0,max,..Default::default()};
            assert_eq!(axis.pressure(0),Some(0.));
            assert_eq!(axis.pressure(max as u32),Some(1.));
            assert!((axis.pressure((max/2) as u32).unwrap()-0.5).abs()<0.001);
        }
    }
    #[test]
    fn native_abi_layout_and_optional_packet_fields() {
        assert_eq!(std::mem::size_of::<LogContext>(), 172);
        assert_eq!(std::mem::size_of::<Axis>(), 16);
        let p = decode(&[0, 10, 2, 1, (-120i32) as u32, 400, 512], BASE | PRESSURE).unwrap();
        assert_eq!(p.x, -120);
        assert_eq!(p.pressure, Some(512));
        assert!(p.orientation.is_none());
        let p = decode(&[0, 10, 2, 1, 30, 40, 900, 450, 1200], BASE | ORIENTATION).unwrap();
        assert_eq!(p.orientation, Some([900, 450, 1200]));
        assert!(p.pressure.is_none());
        assert!(stride(BASE | 1).is_none());
        assert!(decode(&[1], BASE).is_none());
    }
    #[test]
    fn bamboo_pressure_and_intuos_orientation_are_normalized_from_capabilities() {
        let pressure = Axis {
            min: 0,
            max: 1023,
            ..Default::default()
        };
        assert_eq!(pressure.pressure(1023), Some(1.));
        let xp_pen = Axis {
            max: 8191,
            ..pressure
        };
        assert_eq!(xp_pen.pressure(8191), Some(1.));
        assert!((xp_pen.pressure(4096).unwrap() - 0.5).abs() < 0.001);
        assert!(Axis::default().pressure(100).is_none());
        let axis = Axis {
            min: 0,
            max: 3600,
            units: 3,
            resolution: 3600 * 65536,
        };
        let axes = [axis; 3];
        let xy = plane_tilt(axes, [900, 450, 1200]).unwrap();
        assert!((xy[0] - 45.).abs() < 0.001 && xy[1].abs() < 0.001);
        assert_eq!(axis.degrees(1200), Some(120.));
        assert!(plane_tilt([Axis::default(); 3], [0; 3]).is_none());
    }
}
