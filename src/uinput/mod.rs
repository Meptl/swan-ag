#![allow(dead_code)]
#![allow(non_camel_case_types)]
/// A very small, restricted subset of uinput. Not too much work was done to refine this since
/// there exists a uinput crate.
/// That crate currently has an issue compiling (05/27)
mod key;
mod uinput_sys;

pub use self::key::Key;
use self::uinput_sys as ffi;
use std::io::Write;
use std::fs::{OpenOptions, File};
use std::os::unix::io::AsRawFd;

mod ioctl {
    const UINPUT_IOCTL_BASE: u8 = 'U' as u8;

    ioctl!(none ui_dev_create with UINPUT_IOCTL_BASE, 1);
    ioctl!(none ui_dev_destroy with UINPUT_IOCTL_BASE, 2);
    ioctl!(write_ptr set_ev_bit with UINPUT_IOCTL_BASE, 100; ::libc::c_int);
    ioctl!(write_ptr set_key_bit with UINPUT_IOCTL_BASE, 101; ::libc::c_int);
    ioctl!(write_ptr set_rel_bit with UINPUT_IOCTL_BASE, 102; ::libc::c_int);
    ioctl!(write_ptr set_abs_bit with UINPUT_IOCTL_BASE, 103; ::libc::c_int);
}

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts(
        (p as *const T) as *const u8,
        ::std::mem::size_of::<T>(),
    )
}

pub struct UInput {
    ffi: ffi::uinput_user_dev,
    ev: ffi::input_event,
    uinput_device: File,
}

impl UInput {
    /// Create a uinput handle.
    pub fn new() -> UInput {
        let mut name = [0; 80];
        name[0] = 97; // 'a'
        let ffi_dev = ffi::uinput_user_dev {
            name: name,
            id: ffi::input_id {
                bustype: 0x03, // BUS_USB
                vendor: 1,
                product: 1,
                version: 1
            },
            ff_effects_max: 0,
            absmax: [0; 64],
            absmin: [0; 64],
            absfuzz: [0; 64],
            absflat: [0; 64],
        };

        let ev = ffi::input_event {
            time: ffi::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            kind: 0,
            code: 0,
            value: 0,
        };

        // Attempt to open uinput
        let mut uinput_device = OpenOptions::new()
                                 .read(true)
                                 .write(true)
                                 .open("/dev/uinput") // or /dev/input/uinput
                                 .expect("Failed to open uinput");
        let fd = uinput_device.as_raw_fd();

        // Register all relevant events
        unsafe {
            ioctl::set_ev_bit(fd, (EventType::EV_KEY as u8) as *const ::libc::c_int).expect("ioctl failed.");
            ioctl::set_key_bit(fd, 0x110 as *const _).expect("ioctl failed."); // BTN_LEFT
            ioctl::set_key_bit(fd, 0x111 as *const _).expect("ioctl failed."); // BTN_RIGHT
            /*
            ioctl::set_key_bit(fd, 0x112 as *const _).expect("ioctl failed."); // BTN_MIDDLE
            ioctl::set_key_bit(fd, 0x115 as *const _).expect("ioctl failed."); // BTN_FORWARD
            ioctl::set_key_bit(fd, 0x116 as *const _).expect("ioctl failed."); // BTN_BACK
            */
            for i in 1..150u8 {
                ioctl::set_key_bit(fd, i as *const _).expect("ioctl failed."); // Most of the keyboard keys.
            }
            ioctl::set_ev_bit(fd, (EventType::EV_REL as u8) as *const ::libc::c_int).expect("ioctl failed.");
            ioctl::set_rel_bit(fd, 0 as *const _).expect("ioctl failed."); // REL_X
            ioctl::set_rel_bit(fd, 1 as *const _).expect("ioctl failed."); // REL_Y
            /*
            ioctl::set_ev_bit(fd, (EventType::EV_ABS as u8) as *const ::libc::c_int).expect("ioctl failed.");
            ioctl::set_abs_bit(fd, 0 as *const _).expect("ioctl failed."); // ABS_X
            ioctl::set_abs_bit(fd, 1 as *const _).expect("ioctl failed."); // ABS_Y
            */

            let raw_dev = any_as_u8_slice(&ffi_dev);
            uinput_device.write_all(raw_dev).expect("Write failed.");

            ioctl::ui_dev_create(fd).expect("uidev create failed.");
        }

        UInput {
            ffi: ffi_dev,
            ev: ev,
            uinput_device: uinput_device,
        }
    }

    pub fn key_press(&mut self, key: Key) {
        self.ev.kind = EventType::EV_KEY as u16;
        let val: u8 = key.into();
        self.ev.code = val as u16;
        self.ev.value = 1;
        self.write();
    }

    pub fn key_release(&mut self, key: Key) {
        self.ev.kind = EventType::EV_KEY as u16;
        let val: u8 = key.into();
        self.ev.code = val as u16;
        self.ev.value = 0;
        self.write();
    }
    pub fn key_click(&mut self, key: Key) {
        self.key_press(key);
        self.key_release(key);
    }

    pub fn btn_left_press(&mut self) {
        self.ev.kind = EventType::EV_KEY as u16;
        self.ev.code = 0x110 as u16;
        self.ev.value = 1;
        self.write();
    }

    pub fn btn_left_release(&mut self) {
        self.ev.kind = EventType::EV_KEY as u16;
        self.ev.code = 0x110 as u16;
        self.ev.value = 0;
        self.write();
    }

    pub fn btn_right_press(&mut self) {
        self.ev.kind = EventType::EV_KEY as u16;
        self.ev.code = 0x111 as u16;
        self.ev.value = 1;
        self.write();
    }

    pub fn btn_right_release(&mut self) {
        self.ev.kind = EventType::EV_KEY as u16;
        self.ev.code = 0x111 as u16;
        self.ev.value = 0;
        self.write();
    }

    pub fn sync(&mut self) {
        self.ev.kind = EventType::EV_SYN as u16;
        self.ev.code = 0;
        self.ev.value = 0;
        self.write();
    }

    pub fn rel_x(&mut self, val: i32) {
        self.ev.kind = EventType::EV_REL as u16;
        self.ev.code = 0;
        self.ev.value = val;
        self.write();
    }

    pub fn rel_y(&mut self, val: i32) {
        self.ev.kind = EventType::EV_REL as u16;
        self.ev.code = 1;
        self.ev.value = val;
        self.write();
    }

    pub fn abs_x(&mut self, val: i32) {
        self.ev.kind = EventType::EV_ABS as u16;
        self.ev.code = 0;
        self.ev.value = val;
        self.write();
    }

    pub fn abs_y(&mut self, val: i32) {
        self.ev.kind = EventType::EV_ABS as u16;
        self.ev.code = 1;
        self.ev.value = val;
        self.write();
    }

    fn write(&mut self) {
        unsafe {
            let raw_ev = any_as_u8_slice(&self.ev);
            self.uinput_device.write_all(raw_ev).expect("Write failed.");
        }
    }
}

impl Drop for UInput {
    fn drop(&mut self) {
        let fd = self.uinput_device.as_raw_fd();
        unsafe {
            ioctl::ui_dev_destroy(fd).expect("uidev destroy failed.");
        }
    }
}

pub enum EventType {
    EV_SYN = 0x00,
    EV_KEY = 0x01,
    EV_REL = 0x02,
    EV_ABS = 0x03,
    /*
    EV_MSC = 0x04,
    EV_SW =	0x05,
    EV_LED = 0x11,
    EV_SND = 0x12,
    EV_REP = 0x14,
    EV_FF =	0x15,
    EV_PWR = 0x16,
    EV_FF_STATUS = 0x17,
    EV_MAX = 0x1f,
    EV_CNT = 0x20,
    */
}
