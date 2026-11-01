use std::io;

use objc2_application_services::AXError;
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
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

// https://github.com/tasuren/window-observer-rs/blob/6981559652fdefe656926814f81464c5c23046d4/src/platform_impl/macos/helper.rs
#[repr(i32)]
#[derive(Clone, Copy, Debug, ThisError)]
pub enum AccessibilityError {
    #[error("assistive applications are not enabled in System Preferences")]
    ApiDisabled = AXError::APIDisabled.0,
    #[error("the referenced action is not supported")]
    ActionUnsupported = AXError::ActionUnsupported.0,
    #[error("the referenced attribute is not supported")]
    AttributeUnsupported = AXError::AttributeUnsupported.0,
    #[error(
        "a fundamental error has occurred, such as a failure to allocate memory during processing"
    )]
    CannotComplete = AXError::CannotComplete.0,
    #[error("a system error occurred, such as the failure to allocate an object")]
    Failure = AXError::Failure.0,
    #[error(
        "the value received in this event is an invalid value for this attribute, or there are invalid parameters in parameterized attributes"
    )]
    IllegalArgument = AXError::IllegalArgument.0,
    #[error("the accessibility object received in this event is invalid")]
    InvalidUiElement = AXError::InvalidUIElement.0,
    #[error("the observer for the accessibility object received in this event is invalid")]
    InvalidUiElementObserver = AXError::InvalidUIElementObserver.0,
    #[error("the requested value or AXUIElementRef does not exist")]
    NoValue = AXError::NoValue.0,
    #[error("not enough precision")]
    NotEnoughPrecision = AXError::NotEnoughPrecision.0,
    #[error(
        "the function or method is not implemented, or this process does not support the accessibility API"
    )]
    NotImplemented = AXError::NotImplemented.0,
    #[error("this notification has already been registered for")]
    NotificationAlreadyRegistered = AXError::NotificationAlreadyRegistered.0,
    #[error("indicates that a notification is not registered yet")]
    NotificationNotRegistered = AXError::NotificationNotRegistered.0,
    #[error("the notification is not supported by the AXUIElementRef")]
    NotificationUnsupported = AXError::NotificationUnsupported.0,
    #[error("the parameterized attribute is not supported")]
    ParameterizedAttributeUnsupported = AXError::ParameterizedAttributeUnsupported.0,
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
        Ok(match e {
            AXError::Success => return Err(()),

            AXError::APIDisabled => Self::ApiDisabled,
            AXError::ActionUnsupported => Self::ActionUnsupported,
            AXError::AttributeUnsupported => Self::AttributeUnsupported,
            AXError::CannotComplete => Self::CannotComplete,
            AXError::Failure => Self::Failure,
            AXError::IllegalArgument => Self::IllegalArgument,
            AXError::InvalidUIElement => Self::InvalidUiElement,
            AXError::InvalidUIElementObserver => Self::InvalidUiElementObserver,
            AXError::NoValue => Self::NoValue,
            AXError::NotEnoughPrecision => Self::NotEnoughPrecision,
            AXError::NotImplemented => Self::NotImplemented,
            AXError::NotificationAlreadyRegistered => Self::NotificationAlreadyRegistered,
            AXError::NotificationNotRegistered => Self::NotificationNotRegistered,
            AXError::NotificationUnsupported => Self::NotificationUnsupported,
            AXError::ParameterizedAttributeUnsupported => Self::ParameterizedAttributeUnsupported,

            _ => unreachable!(),
        })
    }
}
