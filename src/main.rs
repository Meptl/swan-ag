#[macro_use] extern crate nix;
extern crate input;
extern crate libc;
extern crate libudev_sys;
extern crate time;

mod uinput;

use input::{AsRaw, Libinput, LibinputInterface};
use input::Event::{Keyboard, Pointer};
use input::event::Event;
use input::event::KeyboardEvent::Key;
use input::event::PointerEvent::{Button, Motion};
use input::event::keyboard::{KeyboardEventTrait, KeyState};
use input::event::pointer::PointerEventTrait;
use libc::{c_char, c_int, c_void};
use uinput::UInput;

const SEAT_NAME: &'static str = "seat0";
const RECORD_KEY: uinput::Key = uinput::Key::Esc;
const REPLAY_KEY: uinput::Key = uinput::Key::F2;
static INTERFACE: LibinputInterface = LibinputInterface {
    open_restricted: Some(open_restricted),
    close_restricted: Some(close_restricted),
};

extern fn open_restricted(path: *const c_char, flags: c_int, user_data: *mut c_void) -> c_int {
    // We avoid creating a Rust File because that requires abiding by Rust lifetimes.
    unsafe {
        let fd = ::libc::open(path, flags);
        if fd < 0 {
            println!("open_restricted failed.");
        }
        fd
    }
}

extern fn close_restricted(fd: c_int, user_data: *mut c_void) {
    unsafe {
        libc::close(fd);
    }
}

unsafe fn libinput_from_udev() -> Libinput {
    let udev = libudev_sys::udev_new();
    if udev.is_null() {
        panic!("Could not create udev context.");
    }

    let mut libinput = Libinput::new_from_udev::<&str>(INTERFACE, None, udev as *mut _);
    if libinput.as_raw().is_null() {
        panic!("Failed to create libinput context.");
    }

    libinput.udev_assign_seat(SEAT_NAME).ok();

    libudev_sys::udev_unref(udev);

    libinput
}

/// Replay events in the event store.
/// Modifies the pointer position.
fn replay_events(events: &Vec<Event>, uinput: &mut UInput) {
    println!("Replay!");
    let mut prev_event_time = 0;
    let mut pointer_err = (0_f64, 0_f64); // Total accumulated positional error

    for e in events {
        // Not ideal, but can't get time on generic Event (Devices don't have a time)
        // Each event should assign this value to msec time
        let mut time = prev_event_time;

        match e {
            &Keyboard(Key(ref key_event)) => {
                let key = key_event.key();
                time = key_event.time_usec() / 1000;

                match key_event.key_state() {
                    KeyState::Pressed => {
                        uinput.key_press(uinput::Key::from(key as u8));
                    },
                    KeyState::Released => {
                        uinput.key_release(uinput::Key::from(key as u8));
                    },
                }
            },
            &Pointer(Motion(ref motion_event)) => {
                // This assumes that the units from libinput are the same as that of the uinput
                // device. This is WRONG and doesn't work for some devices. (i.e. my touchpad)
                // Using accelerated data makes touchpads work slightly better but makes worse
                // mouse control.
                let x = motion_event.dx_unaccelerated();
                let y = motion_event.dy_unaccelerated();
                //println!("Rel {} {}", x, y);
                time = motion_event.time_usec() / 1000;

                uinput.rel_x(x as i32);
                uinput.rel_y(y as i32);

                // Though unaccelerated data is typically integers.
                pointer_err.0 += x - ((x as i32) as f64);
                pointer_err.1 += y - ((y as i32) as f64);

                if pointer_err.0.abs() > 1.0 {
                    uinput.rel_x(pointer_err.0 as i32);             // Sends 1 or -1
                    pointer_err.0 -= (pointer_err.0 as i32) as f64; // Subtracts 1 or -1.
                }
                if pointer_err.1.abs() > 1.0 {
                    uinput.rel_y(pointer_err.1 as i32);             // Sends 1 or -1
                    pointer_err.1 -= (pointer_err.1 as i32) as f64; // Subtracts 1 or -1.
                }
            },
            &Pointer(Button(ref button_event)) => {
                let button = button_event.button();
                let value = button_event.seat_button_count();

                match (button, value) {
                    (0x110, 0) => uinput.btn_left_release(),
                    (0x110, 1) => uinput.btn_left_press(),
                    (0x111, 0) => uinput.btn_right_release(),
                    (0x111, 1) => uinput.btn_right_press(),
                    _ => println!("Unimplemented button event!")
                }
            },
            _ => {},
        }
        // Sleep for event delta time then send event
        // Sometimes events become unordered and time is off.
        if prev_event_time != 0 && prev_event_time < time {
            std::thread::sleep(std::time::Duration::from_millis(time - prev_event_time));
        }
        prev_event_time = time;

        // For some events like motion, this is not necessary.
        uinput.sync();
    }
}

fn main() {
    let mut libinput = unsafe { libinput_from_udev() };
    let mut uinput = uinput::UInput::new();
    let mut event_store = Vec::new();

    let mut recording = false;

    println!("Ready!");
    loop {
        if let Err(_) = libinput.dispatch() {
            panic!("libinput dispatch failed.");
        }
        // This dispatch doesn't block and causes a busy loop. For now lets just sleep.
        std::thread::sleep(std::time::Duration::from_millis(50));

        while let Some(event) = libinput.next() {
            match event {
                Keyboard(Key(key_event)) => {
                    let key = key_event.key();
                    let key_state = key_event.key_state();

                    match uinput::Key::from(key as u8) {
                        RECORD_KEY => {
                            if key_state == KeyState::Released {
                                recording = !recording;

                                if recording {
                                    // Flush the event_store
                                    event_store.clear();
                                    println!("Now recording.");
                                } else {
                                    println!("Stopped recording.");
                                }
                            }
                        },
                        REPLAY_KEY => {
                            if key_state == KeyState::Released {
                                recording = false;

                                libinput.suspend();
                                replay_events(&event_store, &mut uinput);
                                if libinput.resume().is_err() {
                                    panic!("Failed to resume libinput");
                                }
                            }
                        },
                        _ => if recording {
                            event_store.push(Keyboard(Key(key_event)));
                        },
                    }

                    //println!("Key {} {:?} [+{}ms]", key, key_state, (time - prev_event_time) / 1000);
                },
                e => {
                    if recording {
                        event_store.push(e);
                    }
                },
            }
        }
    }
}
