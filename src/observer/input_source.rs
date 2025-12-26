use std::{
    cell::UnsafeCell,
    collections::HashMap,
    marker::{PhantomData, PhantomPinned},
    ptr::NonNull,
    sync::{Arc, Mutex},
};

use objc2::Message;
use objc2_core_foundation::{CFArray, CFData, CFDictionary, CFString};
use tracing::info;

#[must_use]
#[derive(Default, Clone, Debug)]
pub struct InputSourceState(Arc<Mutex<HashMap<String, String>>>);

impl InputSourceState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn save(&self, bundle_id: String, input_source: String) {
        self.0.lock().unwrap().insert(bundle_id, input_source);
    }

    pub fn load(&self, bundle_id: &str) -> Option<String> {
        self.0.lock().unwrap().get(bundle_id).map(ToOwned::to_owned)
    }
}

// https://github.com/mzp/EmojiIM/issues/27#issue-1361876711
#[must_use]
pub fn input_source() -> String {
    unsafe {
        let src = TISCopyCurrentKeyboardInputSource();
        let src_id =
            TISGetInputSourceProperty(src, kTISPropertyInputSourceID.as_ptr()) as *const CFString;
        CFString::retain(src_id.as_ref().unwrap()).to_string()
    }
}

// https://github.com/daipeihust/im-select/blob/83046bb75333e58c9a7cbfbd055db6f360361781/macOS/im-select/im-select/main.m
pub fn set_input_source(id: &str) -> bool {
    if input_source() == id {
        return true;
    }
    info!("restoring current input source to `{id}`");
    unsafe {
        let filter = CFDictionary::from_slices(
            &[kTISPropertyInputSourceID.as_ref()],
            &[&*CFString::from_str(id)],
        );
        let srcs = CFArray::<TISInputSource>::retain(
            TISCreateInputSourceList(&raw const *filter, false)
                .as_ref()
                .unwrap(),
        );
        let Some(src) = srcs.get(0) else {
            return false;
        };
        TISSelectInputSource(&*src);
    }
    true
}

#[repr(C)]
pub struct TISInputSource {
    inner: [u8; 0],
    _p: UnsafeCell<PhantomData<(*const UnsafeCell<()>, PhantomPinned)>>,
}

objc2_core_foundation::cf_type! {
    unsafe impl TISInputSource {}
}
objc2::cf_objc2_type! {
    unsafe impl RefEncode<"__TISInputSource"> for TISInputSource {}
}

type OSStatus = i32;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCopyCurrentKeyboardInputSource() -> *const TISInputSource;
    fn TISGetInputSourceProperty(
        source: *const TISInputSource,
        propertyKey: *const CFString,
    ) -> *const CFData;
    fn TISCreateInputSourceList(
        properties: *const CFDictionary<CFString, CFString>,
        includeAllInstalled: bool,
    ) -> *const CFArray<TISInputSource>;
    fn TISSelectInputSource(source: *const TISInputSource) -> OSStatus;

    static kTISPropertyInputSourceID: NonNull<CFString>;
    pub static kTISNotifySelectedKeyboardInputSourceChanged: NonNull<String>;
}
