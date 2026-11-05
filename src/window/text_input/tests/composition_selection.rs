use super::super::editing;
use gpui::{
    Bounds, Context, ElementInputHandler, EntityInputHandler, IntoElement, Pixels,
    PlatformInputHandler, Point, Render, TestAppContext, UTF16Selection, Window, div,
};
use std::ops::Range;

struct Document {
    text: Vec<u16>,
    marked: Option<Range<usize>>,
    selection: Range<usize>,
}

impl Render for Document {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl EntityInputHandler for Document {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        *adjusted = Some(range.clone());
        String::from_utf16(self.text.get(range)?).ok()
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.selection.clone(),
            reversed: false,
        })
    }
    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.selection = range;
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone()
    }
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        let range = range.unwrap_or_else(|| self.selection.clone());
        let end = range.start + text.encode_utf16().count();
        self.text.splice(range, text.encode_utf16());
        self.selection = end..end;
        self.marked = None;
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        _: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        panic!("this test starts with a composition already in the document");
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
fn unmark_preserves_a_document_caret_and_emoji_selection(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Document {
        text: "前😀日本後".encode_utf16().collect(),
        marked: Some(1..5),
        selection: 1..3,
    });
    let mut handler = cx.update(|window, cx| {
        PlatformInputHandler::new(
            window.to_async(cx),
            Box::new(ElementInputHandler::new(Bounds::default(), view.clone())),
        )
    });
    for selection in [1..3, 4..4] {
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.marked = Some(1..5);
                view.selection = selection.clone();
            })
        });
        editing::unmark_text(&mut handler);
        assert_eq!(handler.marked_text_range(), None);
        assert_eq!(
            handler
                .selected_text_range(false)
                .map(|selection| selection.range),
            Some(selection)
        );
        assert_eq!(
            view.read_with(cx, |view, _| view.text.clone()),
            "前😀日本後".encode_utf16().collect::<Vec<_>>()
        );
    }
}
