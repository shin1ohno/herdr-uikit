//! A minimal GPUI app on herdr-uikit: one window with a solid background, a
//! line of text, and a counter that a tap or click increments. It exercises the
//! whole platform (scenes, Metal, Core Text, touch) in the iPad simulator,
//! from an app bundle that herdr-gpui's tooling builds; from a herdr-gpui
//! checkout:
//!
//! ```sh
//! just ios-sim hello
//! ```
//!
//! On every other target `main` returns at once.

#[cfg(target_os = "ios")]
mod hello {
    use gpui::{App, ClickEvent, Context, Window, WindowOptions, div, prelude::*, rgb};

    struct Hello {
        taps: u32,
    }

    impl Hello {
        fn tapped(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
            self.taps = self.taps.saturating_add(1);
            cx.notify();
        }
    }

    impl Render for Hello {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .justify_center()
                .items_center()
                .gap_6()
                .bg(rgb(0x1f3a5f))
                .text_color(rgb(0xffffff))
                .text_3xl()
                .child("Hello from GPUI on iPadOS")
                .child(
                    div()
                        .id("counter")
                        .px_8()
                        .py_4()
                        .rounded_lg()
                        .bg(rgb(0xd0368a))
                        .cursor_pointer()
                        .on_click(cx.listener(Self::tapped))
                        .child(format!("Taps: {}", self.taps)),
                )
        }
    }

    pub fn run() {
        herdr_uikit::application().run(|cx: &mut App| {
            let opened = cx.open_window(WindowOptions::default(), |_, cx| {
                cx.new(|_| Hello { taps: 0 })
            });
            if let Err(error) = opened {
                // Printed to stderr, which herdr-gpui's scripts/ios/sim.sh
                // keeps in stderr.log; the example installs no tracing
                // subscriber.
                eprintln!("hello: could not open a window: {error:#}");
                cx.quit();
            }
        });
    }
}

fn main() {
    #[cfg(target_os = "ios")]
    hello::run();
}
