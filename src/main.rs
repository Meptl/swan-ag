#[macro_use] extern crate nix;
extern crate input;
extern crate libc;
extern crate libudev_sys;
extern crate time;

mod uinput;

use input::{AsRaw, Libinput, LibinputInterface};
use input::Event::Keyboard;
use input::event::Event;
use input::event::KeyboardEvent::Key;
use input::event::keyboard::{KeyboardEventTrait, KeyState};
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

fn replay_events(events: &Vec<Event>, uinput: &mut UInput) {
    println!("Replay!");

    for e in events {
        match e {
            &Keyboard(Key(ref key_event)) => {
                let key = key_event.key();
                match key_event.key_state() {
                    KeyState::Pressed => {
                        uinput.key_press(uinput::Key::from(key as u8));
                    },
                    KeyState::Released => {
                        uinput.key_release(uinput::Key::from(key as u8));
                    },
                }
            },
            _ => println!("Unknown event in event store!")
        }
        uinput.sync();
    }
}


fn main() {
    let mut libinput = unsafe { libinput_from_udev() };
    let mut uinput = uinput::UInput::new();
    let mut event_store = Vec::new();

    let mut recording = false;
    let mut prev_event_time = 0;

    loop {
        if let Err(_) = libinput.dispatch() {
            panic!("libinput dispatch failed.");
        }

        while let Some(event) = libinput.next() {
            match event {
                Keyboard(Key(key_event)) => {
                        let key = key_event.key();
                        let key_state = key_event.key_state();
                        let time = key_event.time_usec();

                        // Perhaps check if prev_event_time was 0;

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

                                    // Get current mouse position and create an AbsMove to it in
                                    // event_store.
                                }
                            },
                            REPLAY_KEY => {
                                if key_state == KeyState::Released {
                                    recording = false;
                                    replay_events(&event_store, &mut uinput);
                                }
                            },
                            _ => if recording {
                                event_store.push(Keyboard(Key(key_event)));
                                prev_event_time = time;
                            },
                        }

                        //println!("Key {} {:?} [+{}ms]", key, key_state, (time - prev_event_time) / 1000);

                },
                _ => {},
            }
        }
    }
}
