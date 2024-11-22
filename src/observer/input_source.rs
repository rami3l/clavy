use std::{
    collections::HashMap,
    ffi::c_void,
    sync::{Arc, Mutex},
};

use core_foundation::{
    array::{CFArray, CFArrayRef},
    base::{CFTypeID, FromVoid, OSStatus, TCFType, ToVoid},
    data::CFDataRef,
    declare_TCFType,
    dictionary::{CFDictionary, CFDictionaryRef},
    impl_TCFType,
    string::{CFString, CFStringRef},
};
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
        let src_id = TISGetInputSourceProperty(src, kTISPropertyInputSourceID) as CFStringRef;
        CFString::wrap_under_get_rule(src_id).to_string()
    }
}

// https://github.com/daipeihust/im-select/blob/83046bb75333e58c9a7cbfbd055db6f360361781/macOS/im-select/im-select/main.m
pub fn set_input_source(id: &str) -> bool {
    if input_source() == id {
        return true;
    }
    info!("restoring current input source to `{id}`");
    unsafe {
        let filter = CFDictionary::from_CFType_pairs(&[(
            CFString::from_void(kTISPropertyInputSourceID.cast()).clone(),
            CFString::new(id),
        )]);
        let srcs = CFArray::<TISInputSource>::wrap_under_get_rule(TISCreateInputSourceList(
            filter.to_untyped().to_void().cast(),
            false,
        ));
        let Some(src) = srcs.get(0) else {
            return false;
        };
        TISSelectInputSource(src.as_concrete_TypeRef());
    }
    true
}

#[derive(Debug)]
#[repr(transparent)]
pub struct __TISInputSource(c_void);
pub type TISInputSourceRef = *const __TISInputSource;

declare_TCFType!(TISInputSource, TISInputSourceRef);
impl_TCFType!(TISInputSource, TISInputSourceRef, TISInputSourceGetTypeID);

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISInputSourceGetTypeID() -> CFTypeID;
    fn TISCopyCurrentKeyboardInputSource() -> TISInputSourceRef;
    fn TISGetInputSourceProperty(source: TISInputSourceRef, propertyKey: CFStringRef) -> CFDataRef;
    fn TISCreateInputSourceList(
        properties: CFDictionaryRef,
        includeAllInstalled: bool,
    ) -> CFArrayRef;
    fn TISSelectInputSource(source: TISInputSourceRef) -> OSStatus;

    static kTISPropertyInputSourceID: CFStringRef;
    pub static kTISNotifySelectedKeyboardInputSourceChanged: CFStringRef;
}
