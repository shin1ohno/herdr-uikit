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
        atomic::{AtomicBool, AtomicU64, Ordering},
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
static EPOCH: AtomicU64 = AtomicU64::new(0);

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
    let epoch = EPOCH.load(Ordering::Acquire);
    let callback = Arc::new(callback);
    notification_authorization(move |state| {
        if let Err(error) = require_authorization(|| state) {
            POSTING.store(false, Ordering::Release);
            callback(Err(error));
            return;
        }
        let callback = callback.clone();
        let notification = notification.clone();
        dispatch2::DispatchQueue::main().exec_async(move || {
            if EPOCH.load(Ordering::Acquire) != epoch {
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
            UNUserNotificationCenter::currentNotificationCenter()
                .addNotificationRequest_withCompletionHandler(
                    &request,
                    Some(&RcBlock::new(move |error: *mut NSError| {
                        POSTING.store(false, Ordering::Release);
                        let failed = !error.is_null();
                        let callback = callback.clone();
                        let tag = notification.tag.to_string();
                        dispatch2::DispatchQueue::main().exec_async(move || {
                            if EPOCH.load(Ordering::Acquire) != epoch {
                                remove_identifiers(&[tag]);
                                callback(Err(NotificationError::Cancelled));
                            } else {
                                callback(if failed {
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
    EPOCH.fetch_add(1, Ordering::AcqRel);
    let tags: Vec<_> = tags.iter().take(64).cloned().collect();
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
            _notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            // The consumer selected OS delivery instead of its own foreground toast.
            completion
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn response(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion: &DynBlock<dyn Fn()>,
        ) {
            let tag = response.notification().request().identifier().to_string();
            let action = response.actionIdentifier().to_string();
            // Dismissals are not activation; unknown actions are forwarded, never guessed.
            if tag.len() <= 128 && action != "com.apple.UNNotificationDismissActionIdentifier" {
                let action_id = (action != "com.apple.UNNotificationDefaultActionIdentifier")
                    .then(|| action.into());
                shared::later(move |shared| {
                    shared.notification_response.call(|f| {
                        f(SystemNotificationResponse {
                            tag: tag.into(),
                            action_id,
                        })
                    });
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
mod tests;
