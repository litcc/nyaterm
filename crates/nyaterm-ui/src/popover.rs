use std::rc::Rc;

use gpui::{Anchor, AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, Window};
use gpui_component::{Selectable, popover::Popover};

type NyaPopoverOpenHandler = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
struct NyaPopoverTrigger {
    element: AnyElement,
    selected: bool,
}

impl Selectable for NyaPopoverTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for NyaPopoverTrigger {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.element
    }
}

#[derive(IntoElement)]
pub struct NyaPopover {
    id: SharedString,
    trigger: AnyElement,
    content: AnyElement,
    anchor: Anchor,
    open: Option<bool>,
    appearance: bool,
    overlay_closable: bool,
    on_open_change: Option<NyaPopoverOpenHandler>,
}

impl NyaPopover {
    pub fn new(
        id: impl Into<SharedString>,
        trigger: impl IntoElement,
        content: impl IntoElement,
    ) -> Self {
        Self {
            id: id.into(),
            trigger: trigger.into_any_element(),
            content: content.into_any_element(),
            anchor: Anchor::TopLeft,
            open: None,
            appearance: true,
            overlay_closable: true,
            on_open_change: None,
        }
    }

    /// Align the popover surface to a corner of its trigger.
    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = Some(open);
        self
    }

    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn overlay_closable(mut self, closable: bool) -> Self {
        self.overlay_closable = closable;
        self
    }

    pub fn on_open_change(
        mut self,
        handler: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for NyaPopover {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let trigger = NyaPopoverTrigger {
            element: self.trigger,
            selected: false,
        };
        let mut popover = Popover::new(self.id)
            .anchor(self.anchor)
            .trigger(trigger)
            .appearance(self.appearance)
            .overlay_closable(self.overlay_closable);
        if let Some(open) = self.open {
            popover = popover.open(open);
        }
        if let Some(on_open_change) = self.on_open_change {
            popover =
                popover.on_open_change(move |open, window, cx| on_open_change(open, window, cx));
        }
        popover.child(self.content)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use gpui::{
        Anchor, Context, InteractiveElement as _, IntoElement as _, Modifiers, ParentElement as _,
        Render, StatefulInteractiveElement as _, Styled as _, TestAppContext, VisualTestContext,
        Window, div,
    };

    use super::NyaPopover;

    struct PopoverFixture {
        popup_clicks: Rc<Cell<usize>>,
        lower_clicks: Rc<Cell<usize>>,
    }

    impl Render for PopoverFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
            let popup_clicks = self.popup_clicks.clone();
            let lower_clicks = self.lower_clicks.clone();
            div()
                .size_full()
                .relative()
                .child(
                    div()
                        .id("popover-lower-layer")
                        .absolute()
                        .inset_0()
                        .on_click(move |_, _, _| lower_clicks.set(lower_clicks.get() + 1)),
                )
                .child(
                    div().w_full().flex().justify_end().child(
                        NyaPopover::new(
                            "anchored-popover",
                            div()
                                .id("anchored-popover-trigger")
                                .debug_selector(|| "anchored-popover-trigger".into())
                                .w_8()
                                .h_8(),
                            div()
                                .id("anchored-popover-content-element")
                                .debug_selector(|| "anchored-popover-content".into())
                                .w_64()
                                .h_16()
                                .on_click(move |_, _, _| popup_clicks.set(popup_clicks.get() + 1)),
                        )
                        .anchor(Anchor::TopRight)
                        .appearance(false),
                    ),
                )
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    #[test]
    fn anchor_defaults_to_top_left_and_can_be_overridden() {
        let default = NyaPopover::new("default", div(), div());
        assert_eq!(default.anchor, Anchor::TopLeft);

        let anchored = NyaPopover::new("anchored", div(), div()).anchor(Anchor::TopRight);
        assert_eq!(anchored.anchor, Anchor::TopRight);

        let _ = anchored.into_any_element();
    }

    #[gpui::test]
    fn anchored_popover_opens_above_later_content_and_owns_pointer_input(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let popup_clicks = Rc::new(Cell::new(0));
        let lower_clicks = Rc::new(Cell::new(0));
        let (_, cx) = cx.add_window_view({
            let popup_clicks = popup_clicks.clone();
            let lower_clicks = lower_clicks.clone();
            move |_, _| PopoverFixture {
                popup_clicks,
                lower_clicks,
            }
        });
        let cx: &mut VisualTestContext = cx;
        draw(cx);

        let trigger = cx.debug_bounds("anchored-popover-trigger").unwrap();
        cx.simulate_click(trigger.center(), Modifiers::default());
        draw(cx);

        let content = cx.debug_bounds("anchored-popover-content").unwrap();
        assert!(content.right() <= trigger.right());
        cx.simulate_click(content.center(), Modifiers::default());
        draw(cx);

        assert_eq!(popup_clicks.get(), 1);
        assert_eq!(lower_clicks.get(), 0);
    }
}
