use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("a native sheet is already presented in this window")]
    AlreadyPresented,
    #[error(transparent)]
    Handle(#[from] raw_window_handle::HandleError),
    #[error("this window has no UIKit view controller")]
    NoController,
    #[error("the font picker must open on the main thread")]
    NotMainThread,
    #[error("this window is not hosted by UIKit")]
    NotUIKit,
}
