//! Read Windows Ink packets before winit reduces them to pressure-only touch events.
//! Never consume a Windows message: normal pointer/UI handling remains with winit.
use crate::input::{PenPhase, PenSample};
use std::sync::Mutex;
use windows_sys::Win32::{
    Graphics::Gdi::ScreenToClient,
    UI::{Input::Pointer::*, WindowsAndMessaging::*},
};

static PACKETS: Mutex<Vec<PenSample>> = Mutex::new(Vec::new());
static WINTAB_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn use_wintab(active: bool) {
    WINTAB_ACTIVE.store(active, std::sync::atomic::Ordering::Relaxed);
}

pub fn drain() -> Vec<PenSample> {
    std::mem::take(&mut *PACKETS.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn message_hook(raw: *const std::ffi::c_void) -> bool {
    // winit owns this MSG and guarantees its lifetime for the callback.
    let msg = unsafe { &*(raw as *const MSG) };
    if WINTAB_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    if matches!(msg.message, WM_KILLFOCUS | WM_POINTERCAPTURECHANGED) {
        push(PenSample {
            id: if msg.message == WM_KILLFOCUS {
                0
            } else {
                (msg.wParam & 0xffff) as u32
            },
            phase: PenPhase::Cancel,
            ..Default::default()
        });
        return false;
    }
    if !matches!(
        msg.message,
        WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP | WM_POINTERLEAVE
    ) {
        return false;
    }
    let id = (msg.wParam & 0xffff) as u32;
    let mut current = POINTER_PEN_INFO::default();
    if unsafe { GetPointerPenInfo(id, &mut current) } == 0 {
        return false;
    }
    if msg.message == WM_POINTERLEAVE {
        push(PenSample {
            id,
            phase: PenPhase::Cancel,
            ..Default::default()
        });
        return false;
    }
    let mut history =
        vec![POINTER_PEN_INFO::default(); current.pointerInfo.historyCount.clamp(1, 256) as usize];
    let mut count = history.len() as u32;
    if unsafe { GetPointerPenInfoHistory(id, &mut count, history.as_mut_ptr()) } == 0 {
        history.clear();
        history.push(current);
    } else {
        history.truncate((count as usize).min(history.len()));
        history.reverse(); // Windows returns newest first; the brush needs oldest first.
    }
    for packet in history {
        let flags = packet.pointerInfo.pointerFlags;
        let phase = if flags & POINTER_FLAG_CANCELED != 0 {
            PenPhase::Cancel
        } else if flags & POINTER_FLAG_DOWN != 0 {
            PenPhase::Down
        } else if flags & POINTER_FLAG_UP != 0 {
            PenPhase::Up
        } else if flags & POINTER_FLAG_INCONTACT != 0 {
            PenPhase::Move
        } else {
            PenPhase::Hover
        };
        let screen_position = packet.pointerInfo.ptPixelLocation;
        let mut position = screen_position;
        if unsafe { ScreenToClient(msg.hwnd, &mut position) } == 0 {
            continue;
        }
        let mut client_px = [position.x as f32, position.y as f32];
        let mut device = windows_sys::Win32::Foundation::RECT::default();
        let mut display = windows_sys::Win32::Foundation::RECT::default();
        if unsafe {
            GetPointerDeviceRects(packet.pointerInfo.sourceDevice, &mut device, &mut display)
        } != 0
            && device.right > device.left
            && device.bottom > device.top
        {
            // Retain the pen digitizer's fractional pixels; the integer mouse position loses
            // detail on slow curves. Match winit's mapping across extended iPad displays.
            let x = display.left as f64
                + packet.pointerInfo.ptHimetricLocation.x as f64
                    * (display.right - display.left) as f64
                    / (device.right - device.left) as f64;
            let y = display.top as f64
                + packet.pointerInfo.ptHimetricLocation.y as f64
                    * (display.bottom - display.top) as f64
                    / (device.bottom - device.top) as f64;
            // Some virtual drivers omit himetric data. Reject an inconsistent mapping.
            if (x - screen_position.x as f64).abs() < 3.
                && (y - screen_position.y as f64).abs() < 3.
            {
                client_px = [
                    position.x as f32 + (x - screen_position.x as f64) as f32,
                    position.y as f32 + (y - screen_position.y as f64) as f32,
                ];
            }
        }
        let tilt_mask = PEN_MASK_TILT_X | PEN_MASK_TILT_Y;
        push(PenSample {
            id,
            phase,
            time_ms: packet.pointerInfo.dwTime,
            client_px,
            pressure: (packet.penMask & PEN_MASK_PRESSURE != 0)
                .then_some(packet.pressure.min(1024) as f32 / 1024.0),
            rotation_deg: (packet.penMask & PEN_MASK_ROTATION != 0)
                .then_some(packet.rotation as f32),
            tilt_xy: (packet.penMask & tilt_mask == tilt_mask)
                .then_some([packet.tiltX as f32, packet.tiltY as f32]),
        });
    }
    false
}

pub(crate) fn push(packet: PenSample) {
    let mut queue = PACKETS.lock().unwrap_or_else(|e| e.into_inner());
    if queue.len() >= 2048 {
        // A stalled UI must not join unrelated strokes after dropping their boundaries.
        queue.clear();
        queue.push(PenSample {
            phase: PenPhase::Cancel,
            ..Default::default()
        });
    }
    queue.push(packet);
}
