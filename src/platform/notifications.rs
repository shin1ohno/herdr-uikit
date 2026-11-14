//! Silent, immediate local notifications. Authorization is a separate user action.
#![allow(
    unsafe_code,
    reason = "UserNotifications delegate and non-null callback arguments"
)]
use super::shared;
use block2::{DynBlock, RcBlock};
use gpui::{SystemNotification, SystemNotificationResponse};
use objc2::{
    AllocAnyThread, define_class, msg_send,
    rc::Retained,
    runtime::{Bool, NSObject, ProtocolObject},
};
use objc2_foundation::{NSArray, NSError, NSObjectProtocol, NSString};
use objc2_user_notifications::*;
use std::{
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// OS authorization, never inferred from a source configuration or submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Authorization {
    NotDetermined,
    Denied,
    Authorized,
    Provisional,
    Ephemeral,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationError {
    Permission(Authorization),
    Native,
    Busy,
    InvalidPayload,
    Cancelled,
}
pub type NotificationResult = Result<(), NotificationError>;
static POSTING: AtomicBool = AtomicBool::new(false);
mod presentation;
static PRESENTATIONS: presentation::Registry = presentation::Registry::new();
mod response;
static RESPONSES: response::Registry = response::Registry::new();
#[cfg(any(test, feature = "test-support"))]
mod response_fixture;
#[cfg(feature = "test-support")]
pub use response_fixture::NotificationResponseFixture;

fn require_authorization(provider: impl FnOnce() -> Authorization) -> NotificationResult {
    match provider() {
        Authorization::Authorized => Ok(()),
        state => Err(NotificationError::Permission(state)),
    }
}

fn authorization(value: UNAuthorizationStatus) -> Authorization {
    match value {
        UNAuthorizationStatus::NotDetermined => Authorization::NotDetermined,
        UNAuthorizationStatus::Denied => Authorization::Denied,
        UNAuthorizationStatus::Authorized => Authorization::Authorized,
        UNAuthorizationStatus::Provisional => Authorization::Provisional,
        UNAuthorizationStatus::Ephemeral => Authorization::Ephemeral,
        _ => Authorization::Unknown,
    }
}
/// Queries the actual center asynchronously; never presents a prompt.
pub fn notification_authorization(callback: impl Fn(Authorization) + Send + Sync + 'static) {
    UNUserNotificationCenter::currentNotificationCenter()
        .getNotificationSettingsWithCompletionHandler(&RcBlock::new(
            move |settings: NonNull<UNNotificationSettings>| {
                // SAFETY: The center provides a live non-null settings object for this callback.
                callback(authorization(
                    unsafe { settings.as_ref() }.authorizationStatus(),
                ));
            },
        ));
}
/// Call only from an explicit user action. This function may present an OS dialog.
pub fn request_notification_authorization(
    callback: impl Fn(NotificationResult) + Send + Sync + 'static,
) {
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert,
            &RcBlock::new(move |granted: Bool, error: *mut NSError| {
                callback(if !error.is_null() {
                    Err(NotificationError::Native)
                } else if granted.as_bool() {
                    Ok(())
                } else {
                    Err(NotificationError::Permission(Authorization::Denied))
                });
            }),
        );
}
/// Accepted means UNUserNotificationCenter accepted the request, not that a banner appeared.
/// One request is in flight globally; overload is explicit and never queued/replayed.
pub fn post_notification(
    notification: SystemNotification,
    callback: impl Fn(NotificationResult) + Send + Sync + 'static,
) {
    post_notification_when(notification, || true, callback);
}

/// Rechecks a nonblocking consumer validity predicate after asynchronous OS work.
/// A false predicate cancels presentation, not the response lifecycle. Consumers
/// must validate context and single-use ownership again when handling a response.
pub fn post_notification_when(
    notification: SystemNotification,
    current: impl Fn() -> bool + Send + Sync + 'static,
    callback: impl Fn(NotificationResult) + Send + Sync + 'static,
) {
    if notification.tag.is_empty()
        || notification.tag.len() > 128
        || notification.title.len() > 256
        || notification.body.len() > 1024
        || !notification.actions.is_empty()
    {
        callback(Err(NotificationError::InvalidPayload));
        return;
    }
    if POSTING.swap(true, Ordering::AcqRel) {
        callback(Err(NotificationError::Busy));
        return;
    }
    if RESPONSES.owns(&notification.tag, std::time::Instant::now()) {
        POSTING.store(false, Ordering::Release);
        callback(Err(NotificationError::Busy));
        return;
    }
    let ticket = match PRESENTATIONS.insert(&notification.tag, Arc::new(current)) {
        Ok(ticket) => ticket,
        Err(error) => {
            POSTING.store(false, Ordering::Release);
            callback(Err(error));
            return;
        }
    };
    let callback = Arc::new(callback);
    notification_authorization(move |state| {
        if !ticket.current() {
            PRESENTATIONS.finish(&notification.tag, &ticket);
            POSTING.store(false, Ordering::Release);
            callback(Err(NotificationError::Cancelled));
            return;
        }
        if let Err(error) = require_authorization(|| state) {
            PRESENTATIONS.finish(&notification.tag, &ticket);
            POSTING.store(false, Ordering::Release);
            callback(Err(error));
            return;
        }
        let callback = callback.clone();
        let notification = notification.clone();
        let ticket = ticket.clone();
        dispatch2::DispatchQueue::main().exec_async(move || {
            if !ticket.current() {
                PRESENTATIONS.finish(&notification.tag, &ticket);
                POSTING.store(false, Ordering::Release);
                callback(Err(NotificationError::Cancelled));
                return;
            }
            let content = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str(&notification.title));
            content.setBody(&NSString::from_str(&notification.body));
            content.setSound(None);
            let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
                &NSString::from_str(&notification.tag),
                &content,
                None,
            );
            let callback = callback.clone();
            // Only an authorized request actually submitted to the center owns
            // a response. Record before add: response may precede completion.
            RESPONSES.issue(&notification.tag, &ticket, std::time::Instant::now());
            UNUserNotificationCenter::currentNotificationCenter()
                .addNotificationRequest_withCompletionHandler(
                    &request,
                    Some(&RcBlock::new(move |error: *mut NSError| {
                        POSTING.store(false, Ordering::Release);
                        let failed = !error.is_null();
                        let callback = callback.clone();
                        let tag = notification.tag.to_string();
                        let ticket = ticket.clone();
                        dispatch2::DispatchQueue::main().exec_async(move || {
                            if failed {
                                RESPONSES.forget(&tag, &ticket);
                            }
                            if !ticket.current() {
                                PRESENTATIONS.finish(&tag, &ticket);
                                remove_identifiers(&[tag]);
                                callback(Err(NotificationError::Cancelled));
                            } else {
                                callback(if failed {
                                    PRESENTATIONS.finish(&tag, &ticket);
                                    Err(NotificationError::Native)
                                } else {
                                    Ok(())
                                });
                            }
                        });
                    })),
                );
        });
    });
}
/// Removes only the supplied app-owned identifiers, never other notifications.
pub fn remove_notifications(tags: &[String]) {
    PRESENTATIONS.remove(tags);
    let tags: Vec<_> = tags
        .iter()
        .take(64)
        .filter(|s| s.len() <= 128)
        .cloned()
        .collect();
    dispatch2::DispatchQueue::main().exec_async(move || remove_identifiers(&tags));
}
fn remove_identifiers(tags: &[String]) {
    let strings: Vec<_> = tags
        .iter()
        .take(64)
        .filter(|s| s.len() <= 128)
        .map(|s| NSString::from_str(s))
        .collect();
    let ids = NSArray::from_retained_slice(&strings);
    let center = UNUserNotificationCenter::currentNotificationCenter();
    center.removePendingNotificationRequestsWithIdentifiers(&ids);
    center.removeDeliveredNotificationsWithIdentifiers(&ids);
}

fn complete_presentation(
    ticket: Option<&presentation::Ticket>,
    completion: impl FnOnce(UNNotificationPresentationOptions),
) {
    let options = if ticket.is_some_and(presentation::Ticket::current) {
        UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List
    } else {
        UNNotificationPresentationOptions::empty()
    };
    completion(options);
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; this class has no ivars.
    #[unsafe(super(NSObject))]
    pub(super) struct NotificationDelegate;
    unsafe impl NSObjectProtocol for NotificationDelegate {}
    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn present(
            &self,
            _center: &UNUserNotificationCenter,
            notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            let _native_entry = crate::native_operation::refused();
            let tag = notification.request().identifier().to_string();
            // The final decision uses the retained context, not add-request success.
            // Unknown, removed, disconnected or invalidated tags never present.
            let ticket = PRESENTATIONS.get(&tag);
            complete_presentation(ticket.as_deref(), |options| completion.call((options,)));
        }
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn response(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion: &DynBlock<dyn Fn()>,
        ) {
            let _native_entry = crate::native_operation::refused();
            let tag = response.notification().request().identifier().to_string();
            let action = response.actionIdentifier().to_string();
            if tag.len() <= 128 && action.len() <= 128 {
                shared::later(move |shared| {
                    response::dispatch(
                        &PRESENTATIONS,
                        &RESPONSES,
                        &tag,
                        &action,
                        std::time::Instant::now(),
                        |response| {
                            shared.notification_response.call(|f| f(response));
                        },
                    );
                });
            }
            completion.call(());
        }
    }
);
impl NotificationDelegate {
    pub(super) fn register() -> Retained<Self> {
        // SAFETY: NSObject initializer after setting empty ivars.
        let this: Retained<Self> = unsafe { msg_send![super(Self::alloc().set_ivars(())), init] };
        UNUserNotificationCenter::currentNotificationCenter()
            .setDelegate(Some(ProtocolObject::from_ref(&*this)));
        this
    }
}

#[cfg(test)]
mod response_tests;
#[cfg(test)]
mod tests;
