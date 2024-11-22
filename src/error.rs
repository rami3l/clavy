use std::io;

use accessibility_sys::AXError;
use thiserror::Error as ThisError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, ThisError)]
pub enum Error {
    #[error("the $HOME environment variable is not set")]
    HomeNotSet,
    #[error("the current executable path could not be retrieved")]
    FaultyExePath,
    #[error("accessibility privileges are not detected")]
    AxPrivilegesNotDetected,
    #[error(transparent)]
    Io(#[from] io::Error),
}

// https://github.com/tasuren/window-observer-rs/blob/6981559652fdefe656926814f81464c5c23046d4/src/platform_impl/macos/helper.rs
#[derive(Clone, Copy, Debug, ThisError)]
pub enum AccessibilityError {
    #[error("assistive applications are not enabled in System Preferences")]
    APIDisabled(i32),
    #[error("the referenced action is not supported")]
    ActionUnsupported(i32),
    #[error("the referenced attribute is not supported")]
    AttributeUnsupported(i32),
    #[error(
        "a fundamental error has occurred, such as a failure to allocate memory during processing"
    )]
    CannotComplete(i32),
    #[error("a system error occurred, such as the failure to allocate an object")]
    Failure(i32),
    #[error("the value received in this event is an invalid value for this attribute, or there are invalid parameters in parameterized attributes")]
    IllegalArgument(i32),
    #[error("the accessibility object received in this event is invalid")]
    InvalidUIElement(i32),
    #[error("the observer for the accessibility object received in this event is invalid")]
    InvalidUIElementObserver(i32),
    #[error("the requested value or AXUIElementRef does not exist")]
    NoValue(i32),
    #[error("not enough precision")]
    NotEnoughPrecision(i32),
    #[error("the function or method is not implemented, or this process does not support the accessibility API")]
    NotImplemented(i32),
    #[error("this notification has already been registered for")]
    NotificationAlreadyRegistered(i32),
    #[error("indicates that a notification is not registered yet")]
    NotificationNotRegistered(i32),
    #[error("the notification is not supported by the AXUIElementRef")]
    NotificationUnsupported(i32),
    #[error("the parameterized attribute is not supported")]
    ParameterizedAttributeUnsupported(i32),
}

impl AccessibilityError {
    pub fn wrap(e: AXError) -> Result<(), Self> {
        match e.try_into() {
            Ok(e) => Err(e),
            Err(()) => Ok(()),
        }
    }
}

impl TryFrom<AXError> for AccessibilityError {
    type Error = ();

    fn try_from(e: AXError) -> Result<Self, Self::Error> {
        #![allow(non_upper_case_globals)]

        use accessibility_sys::*;
        if e == kAXErrorSuccess {
            return Err(());
        };

        Ok(match e {
            kAXErrorAPIDisabled => Self::APIDisabled(e),
            kAXErrorActionUnsupported => Self::ActionUnsupported(e),
            kAXErrorAttributeUnsupported => Self::AttributeUnsupported(e),
            kAXErrorCannotComplete => Self::CannotComplete(e),
            kAXErrorFailure => Self::Failure(e),
            kAXErrorIllegalArgument => Self::IllegalArgument(e),
            kAXErrorInvalidUIElement => Self::InvalidUIElement(e),
            kAXErrorInvalidUIElementObserver => Self::InvalidUIElementObserver(e),
            kAXErrorNoValue => Self::NoValue(e),
            kAXErrorNotEnoughPrecision => Self::NotEnoughPrecision(e),
            kAXErrorNotImplemented => Self::NotImplemented(e),
            kAXErrorNotificationAlreadyRegistered => Self::NotificationAlreadyRegistered(e),
            kAXErrorNotificationNotRegistered => Self::NotificationNotRegistered(e),
            kAXErrorNotificationUnsupported => Self::NotificationUnsupported(e),
            kAXErrorParameterizedAttributeUnsupported => Self::ParameterizedAttributeUnsupported(e),
            _ => unreachable!(),
        })
    }
}
