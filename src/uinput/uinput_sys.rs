pub type __time_t = ::std::os::raw::c_long;
pub type __suseconds_t = ::std::os::raw::c_long;
pub type __u32 = ::std::os::raw::c_uint;
pub type __s32 = ::std::os::raw::c_int;
pub type __s16 = ::std::os::raw::c_short;
pub type __u16 = ::std::os::raw::c_ushort;

#[repr(C)]
pub struct uinput_user_dev {
    pub name: [::std::os::raw::c_char; 80usize],
    pub id: input_id,
    pub ff_effects_max: __u32,
    pub absmax: [__s32; 64usize],
    pub absmin: [__s32; 64usize],
    pub absfuzz: [__s32; 64usize],
    pub absflat: [__s32; 64usize],
}

#[repr(C)]
pub struct input_id {
    pub bustype: __u16,
    pub vendor: __u16,
    pub product: __u16,
    pub version: __u16,
}

#[repr(C)]
pub struct input_event {
    pub time: timeval,
    pub kind: __u16,
    pub code: __u16,
    pub value: __s32,
}

#[repr(C)]
pub struct timeval {
    pub tv_sec: __time_t,
    pub tv_usec: __suseconds_t,
}
