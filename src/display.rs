//! `gpui::PlatformDisplay` over `UIScreen`. An iPad has its built-in screen
//! and, with Stage Manager or mirroring, possibly one external screen; GPUI
//! sees each screen a window scene can be on.
//!
//! Screens are found through the app's connected window scenes, as UIKit has
//! recommended since iPadOS 16. The device's own screen is always listed as
//! well, because a new scene can open there while none is on it yet; UIKit
//! names it only through the deprecated `UIScreen.mainScreen`.
//!
//! UIKit is main-thread only, so every query takes a `MainThreadMarker`, and
//! an `IosDisplay` holds its `UIScreen`, which keeps the display on the main
//! thread and its bounds current through rotation. The queries need a running
//! application (after `UIApplicationMain`), as every GPUI platform call does.

#[cfg(test)]
mod tests;

use std::{cell::RefCell, ptr};

use gpui::{Bounds, DisplayId, Pixels, PlatformDisplay, point, px, size};
use objc2::{
    MainThreadMarker,
    rc::{Retained, Weak},
};
use objc2_core_foundation::{CGRect, CGSize};
use objc2_ui_kit::{UIApplication, UISceneActivationState, UIScreen, UIWindowScene};
use uuid::Uuid;

/// Namespace of the display uuids (a fixed random v4 uuid), so they cannot
/// equal uuids derived elsewhere from the same names.
const UUID_NAMESPACE: Uuid = Uuid::from_u128(0x6f1c_2d0e_8b4a_4c55_9a3e_7d21_b8f0_c6a4);

thread_local! {
    /// Only the main thread touches this: every path to it holds a
    /// `MainThreadMarker`.
    static SCREEN_IDS: RefCell<IdTable<Weak<UIScreen>>> = const { RefCell::new(IdTable::new()) };
}

#[derive(Debug)]
pub(crate) struct IosDisplay {
    id: DisplayId,
    uuid: Uuid,
    screen: Retained<UIScreen>,
}

impl IosDisplay {
    /// The screen of the app's most active window scene, which is where the
    /// system puts a scene the app asks for; the device's own screen before
    /// any scene connects. Activation state does not tell which of several
    /// foreground-active scenes has focus (Stage Manager can show one on each
    /// screen), so ties go to the device's own screen and then to the screen
    /// the app saw first, and the answer holds while nothing changes.
    #[expect(
        dead_code,
        reason = "the platform module calls this; that module is still a Wave 0 stub"
    )]
    pub(crate) fn primary(mtm: MainThreadMarker) -> Self {
        let builtin = builtin_screen(mtm);
        let screen = scene_screens(&builtin, mtm)
            .into_iter()
            .next()
            .unwrap_or_else(|| builtin.clone());
        Self::with_builtin(screen, &builtin)
    }

    /// Every screen a scene can currently be on: the primary one first, then
    /// the others in the order the app first saw them.
    pub(crate) fn all(mtm: MainThreadMarker) -> Vec<Self> {
        let builtin = builtin_screen(mtm);
        let mut screens = scene_screens(&builtin, mtm);
        if !screens.iter().any(|screen| same_screen(screen, &builtin)) {
            screens.push(builtin.clone());
        }
        let mut displays: Vec<_> = screens
            .into_iter()
            .map(|screen| Self::with_builtin(screen, &builtin))
            .collect();
        if let Some((_primary, others)) = displays.split_first_mut() {
            others.sort_by_key(|display| u64::from(display.id));
        }
        displays
    }

    #[expect(
        dead_code,
        reason = "the window module calls this; that module is still a Wave 0 stub"
    )]
    pub(crate) fn find_by_id(id: DisplayId, mtm: MainThreadMarker) -> Option<Self> {
        Self::all(mtm).into_iter().find(|display| display.id == id)
    }

    /// The display for `screen`, such as the one a window's scene is on.
    #[expect(
        dead_code,
        reason = "for the window module's `PlatformWindow::display`, still a Wave 0 stub"
    )]
    pub(crate) fn for_screen(screen: Retained<UIScreen>, mtm: MainThreadMarker) -> Self {
        Self::with_builtin(screen, &builtin_screen(mtm))
    }

    fn with_builtin(screen: Retained<UIScreen>, builtin: &UIScreen) -> Self {
        let kind = if same_screen(&screen, builtin) {
            ScreenKind::BuiltIn
        } else {
            ScreenKind::External(mode_size(&screen))
        };
        Self {
            id: screen_id(&screen),
            uuid: kind.uuid(),
            screen,
        }
    }
}

impl PlatformDisplay for IosDisplay {
    fn id(&self) -> DisplayId {
        self.id
    }

    fn uuid(&self) -> anyhow::Result<Uuid> {
        Ok(self.uuid)
    }

    // `visible_bounds` keeps the trait's default, the whole screen: iPadOS
    // reserves no screen area that windows must avoid, and the safe-area
    // insets that exist belong to each window, not to the screen.
    fn bounds(&self) -> Bounds<Pixels> {
        bounds_from_rect(self.screen.bounds())
    }
}

/// What a display's persistent uuid is derived from. UIKit gives screens no
/// persistent identifier, so the built-in screen has one fixed uuid and an
/// external screen one per native resolution, which tells a user's usual
/// monitors apart.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ScreenKind {
    BuiltIn,
    External(CGSize),
}

impl ScreenKind {
    fn uuid(self) -> Uuid {
        match self {
            Self::BuiltIn => Uuid::new_v5(&UUID_NAMESPACE, b"builtin"),
            Self::External(CGSize { width, height }) => {
                let name = format!("external:{width:.0}x{height:.0}");
                Uuid::new_v5(&UUID_NAMESPACE, name.as_bytes())
            }
        }
    }
}

/// Session-unique display ids. A screen keeps its id while it stays
/// connected and an id is never handed out twice, so a `DisplayId` GPUI kept
/// from an earlier query cannot come to name a different screen. `K` is a
/// weak handle to the screen.
#[derive(Debug)]
struct IdTable<K> {
    next: u64,
    entries: Vec<(K, DisplayId)>,
}

impl<K> IdTable<K> {
    const fn new() -> Self {
        Self {
            next: 1,
            entries: Vec::new(),
        }
    }

    /// The id of the entry `is_screen` accepts, or a new id recorded for
    /// `key()`. Entries `is_live` rejects (disconnected screens) are dropped
    /// first.
    fn id_for(
        &mut self,
        is_live: impl Fn(&K) -> bool,
        is_screen: impl Fn(&K) -> bool,
        key: impl FnOnce() -> K,
    ) -> DisplayId {
        self.entries.retain(|(known, _)| is_live(known));
        if let Some((_, id)) = self.entries.iter().find(|(known, _)| is_screen(known)) {
            return *id;
        }
        let id = DisplayId::new(self.next);
        self.next += 1;
        self.entries.push((key(), id));
        id
    }
}

fn screen_id(screen: &Retained<UIScreen>) -> DisplayId {
    SCREEN_IDS.with_borrow_mut(|ids| {
        ids.id_for(
            |known| known.load().is_some(),
            |known| {
                known
                    .load()
                    .is_some_and(|known| same_screen(&known, screen))
            },
            || Weak::from_retained(screen),
        )
    })
}

/// The screens the app's window scenes are on, each once, in
/// [`ScreenOrder`]. `connectedScenes` is an unordered set, so nothing here
/// depends on the order it lists scenes in.
fn scene_screens(builtin: &UIScreen, mtm: MainThreadMarker) -> Vec<Retained<UIScreen>> {
    let scenes = UIApplication::sharedApplication(mtm)
        .connectedScenes()
        .to_vec()
        .into_iter()
        .filter_map(|scene| scene.downcast::<UIWindowScene>().ok())
        .map(|scene| {
            let screen = scene.screen();
            let order = ScreenOrder {
                rank: activation_rank(scene.activationState()),
                external: !same_screen(&screen, builtin),
                id: u64::from(screen_id(&screen)),
            };
            (order, screen)
        });
    order_screens(scenes, |a, b| same_screen(a, b))
}

/// Where a screen goes in the display order: by its most active scene, then
/// the device's own screen before an external one, then the screen the app
/// saw first. The id makes the order total, so equal ranks cannot swap
/// between queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ScreenOrder {
    rank: u8,
    external: bool,
    id: u64,
}

/// Each screen of `scenes` once, sorted by [`ScreenOrder`]. A screen that
/// hosts several scenes takes the order of its most active one. `S` is a
/// screen and `same` tells whether two are the same screen.
fn order_screens<S>(
    scenes: impl IntoIterator<Item = (ScreenOrder, S)>,
    same: impl Fn(&S, &S) -> bool,
) -> Vec<S> {
    let mut screens: Vec<(ScreenOrder, S)> = Vec::new();
    for (order, screen) in scenes {
        match screens.iter_mut().find(|(_, known)| same(known, &screen)) {
            Some((known, _)) => *known = (*known).min(order),
            None => screens.push((order, screen)),
        }
    }
    screens.sort_by_key(|(order, _)| *order);
    screens.into_iter().map(|(_, screen)| screen).collect()
}

/// Orders scenes from the one the user is in to the ones they cannot see.
fn activation_rank(state: UISceneActivationState) -> u8 {
    match state {
        UISceneActivationState::ForegroundActive => 0,
        UISceneActivationState::ForegroundInactive => 1,
        UISceneActivationState::Background => 2,
        _ => 3,
    }
}

#[allow(
    deprecated,
    reason = "UIKit names the device's own screen only through `mainScreen`; scenes name just the screens they are on"
)]
fn builtin_screen(mtm: MainThreadMarker) -> Retained<UIScreen> {
    UIScreen::mainScreen(mtm)
}

/// The screen's native resolution in pixels: its preferred mode, which does
/// not change when the user picks another, else its current bounds.
fn mode_size(screen: &UIScreen) -> CGSize {
    screen
        .preferredMode()
        .map_or_else(|| screen.nativeBounds().size, |mode| mode.size())
}

fn same_screen(a: &UIScreen, b: &UIScreen) -> bool {
    ptr::eq(a, b)
}

/// A rectangle in UIKit points, whose origin is already the top left, as
/// GPUI's.
fn bounds_from_rect(rect: CGRect) -> Bounds<Pixels> {
    Bounds {
        origin: point(px(rect.origin.x as f32), px(rect.origin.y as f32)),
        size: size(px(rect.size.width as f32), px(rect.size.height as f32)),
    }
}
