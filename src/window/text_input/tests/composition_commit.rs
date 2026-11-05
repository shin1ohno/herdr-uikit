use super::super::editing;
use gpui::{
    Bounds, Context, ElementInputHandler, EntityInputHandler, IntoElement, Pixels,
    PlatformInputHandler, Point, Render, TestAppContext, UTF16Selection, Window, div,
};
use std::ops::Range;

#[derive(Default)]
struct Terminal {
    marked: String,
    commits: Vec<String>,
}
impl Render for Terminal {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
impl EntityInputHandler for Terminal {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        assert_eq!(range, 0..self.marked.encode_utf16().count());
        *adjusted = Some(range);
        Some(self.marked.clone())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.marked.encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.marked.is_empty()).then(|| 0..self.marked.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked.clear();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.marked.clear();
        if !text.is_empty() {
            self.commits.push(text.into());
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.marked = text.into();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

#[gpui::test]
fn uikit_unmark_commits_a_terminal_composition_exactly_once(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Terminal::default());
    let mut handler = cx.update(|window, cx| {
        PlatformInputHandler::new(
            window.to_async(cx),
            Box::new(ElementInputHandler::new(Bounds::default(), view.clone())),
        )
    });
    for text in ["日本", "𠮷野家"] {
        handler.replace_and_mark_text_in_range(None, text, None);
        assert_eq!(
            handler.marked_text_range(),
            Some(0..text.encode_utf16().count())
        );
        let count = view.read_with(cx, |view, _| view.commits.len());
        editing::unmark_text(&mut handler);
        assert_eq!(
            &view.read_with(cx, |view, _| view.commits.clone())[count..],
            &[text.to_owned()]
        );
        assert_eq!(handler.marked_text_range(), None);
        editing::unmark_text(&mut handler);
        assert_eq!(view.read_with(cx, |view, _| view.commits.len()), count + 1);
    }
}

#[gpui::test]
fn explicit_commit_then_unmark_does_not_duplicate_text(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Terminal::default());
    let mut handler = cx.update(|window, cx| {
        PlatformInputHandler::new(
            window.to_async(cx),
            Box::new(ElementInputHandler::new(Bounds::default(), view.clone())),
        )
    });
    handler.replace_and_mark_text_in_range(None, "にほん", None);
    handler.replace_text_in_range(None, "日本");
    editing::unmark_text(&mut handler);
    assert_eq!(view.read_with(cx, |view, _| view.commits.clone()), ["日本"]);
    handler.replace_and_mark_text_in_range(None, "とりけし", None);
    handler.replace_text_in_range(Some(0..4), "");
    editing::unmark_text(&mut handler);
    assert_eq!(view.read_with(cx, |view, _| view.commits.clone()), ["日本"]);
}

mod control_input;
