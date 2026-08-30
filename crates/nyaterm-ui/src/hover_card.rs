use std::time::Duration;

use gpui::{Anchor, AnyElement, IntoElement, ParentElement as _, RenderOnce, SharedString, Window};
use gpui_component::hover_card::HoverCard;

/// Hover-triggered rich content that stays open while the pointer moves from
/// the trigger into the card.
///
/// This is the stable NyaTerm boundary for `gpui-component`'s HoverCard. It is
/// intended for concise, pointer-interactive previews; click-driven workflows
/// should continue to use [`crate::NyaPopover`].
#[derive(IntoElement)]
pub struct NyaHoverCard {
    id: SharedString,
    trigger: AnyElement,
    content: AnyElement,
    anchor: Anchor,
    open_delay: Duration,
    close_delay: Duration,
    appearance: bool,
}

impl NyaHoverCard {
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
            open_delay: Duration::from_millis(700),
            close_delay: Duration::from_millis(250),
            appearance: true,
        }
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn open_delay(mut self, delay: Duration) -> Self {
        self.open_delay = delay;
        self
    }

    pub fn close_delay(mut self, delay: Duration) -> Self {
        self.close_delay = delay;
        self
    }

    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }
}

impl RenderOnce for NyaHoverCard {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        HoverCard::new(self.id)
            .anchor(self.anchor)
            .open_delay(self.open_delay)
            .close_delay(self.close_delay)
            .appearance(self.appearance)
            .trigger(self.trigger)
            .child(self.content)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use gpui::{
        Context, InteractiveElement as _, Modifiers, Render, StatefulInteractiveElement as _,
        Styled as _, TestAppContext, div, point, px,
    };

    use super::*;

    struct Harness {
        clicked: Rc<Cell<bool>>,
    }

    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let clicked = self.clicked.clone();
            NyaHoverCard::new(
                "hover-card",
                div()
                    .debug_selector(|| "hover-card-trigger".into())
                    .w(px(118.))
                    .h(px(36.)),
                div()
                    .id("hover-card-content")
                    .debug_selector(|| "hover-card-content".into())
                    .size(px(80.))
                    .on_click(move |_, _, _| clicked.set(true)),
            )
            .appearance(false)
            .open_delay(Duration::from_millis(10))
            .close_delay(Duration::from_millis(100))
        }
    }

    #[test]
    fn default_open_delay_avoids_triggering_on_a_brief_pass() {
        let card = NyaHoverCard::new("hover-card-delay", div(), div());
        assert_eq!(card.open_delay, Duration::from_millis(700));
    }

    #[gpui::test]
    fn content_stays_open_and_accepts_clicks_after_leaving_trigger(cx: &mut TestAppContext) {
        let clicked = Rc::new(Cell::new(false));
        let clicked_for_view = clicked.clone();
        let (_, cx) = cx.add_window_view(|_, _| Harness {
            clicked: clicked_for_view,
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let trigger = cx.debug_bounds("hover-card-trigger").unwrap();
        assert_eq!(trigger.size.width, px(118.));
        assert_eq!(trigger.size.height, px(36.));
        cx.simulate_mouse_move(trigger.center(), None, Modifiers::default());
        cx.executor().advance_clock(Duration::from_millis(10));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            window.draw(cx).clear(cx);
        });

        let content = cx.debug_bounds("hover-card-content").unwrap();
        cx.simulate_mouse_move(content.center(), None, Modifiers::default());
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("hover-card-content").is_some());

        cx.simulate_click(content.center(), Modifiers::default());
        assert!(clicked.get());

        cx.simulate_mouse_move(point(px(200.), px(200.)), None, Modifiers::default());
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("hover-card-content").is_none());
    }
}
