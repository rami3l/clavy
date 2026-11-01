use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use objc2_core_foundation::{CFArray, CFDictionary, CFRetained, CFString};
use tracing::info;

use self::types::TISInputSource;

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
pub fn input_source() -> Option<String> {
    unsafe {
        let src = TISCopyCurrentKeyboardInputSource()?;
        TISGetInputSourceProperty(&src, kTISPropertyInputSourceID).map(ToString::to_string)
    }
}

// https://github.com/daipeihust/im-select/blob/83046bb75333e58c9a7cbfbd055db6f360361781/macOS/im-select/im-select/main.m
pub fn set_input_source(id: &str) -> bool {
    if input_source().is_some_and(|s| s == id) {
        return true;
    }
    info!("restoring current input source to `{id}`");
    let filter = CFDictionary::from_slices(
        &[unsafe { kTISPropertyInputSourceID }],
        &[&*CFString::from_str(id)],
    );
    let Some(srcs) = (unsafe { TISCreateInputSourceList(Some(filter.as_opaque()), false) }) else {
        return false;
    };
    let Some(src) = srcs.get(0) else {
        return false;
    };
    unsafe { TISSelectInputSource(&src) };
    true
}

#[expect(clippy::transmute_ptr_to_ptr)]
mod types {
    use std::ffi::c_void;

    #[repr(transparent)]
    pub struct TISInputSource(c_void);

    objc2_core_foundation::cf_type!(
        #[encoding_name = "__TISInputSource"]
        unsafe impl TISInputSource {}
    );
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C-unwind" {
    fn TISCopyCurrentKeyboardInputSource() -> Option<CFRetained<TISInputSource>>;
    fn TISGetInputSourceProperty<'a>(
        source: &'a TISInputSource,
        property_key: &CFString,
    ) -> Option<&'a CFString>;
    fn TISCreateInputSourceList(
        properties: Option<&CFDictionary>,
        include_all_installed: bool,
    ) -> Option<CFRetained<CFArray<TISInputSource>>>;
    fn TISSelectInputSource(source: &TISInputSource) -> i32;

    static kTISPropertyInputSourceID: &'static CFString;
    pub static kTISNotifySelectedKeyboardInputSourceChanged: &'static CFString;
}
