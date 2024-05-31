use gpui::*;

struct HelloWorld {
    text: SharedString,
}

impl Render for HelloWorld {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        let ls = ListState::new(10, ListAlignment::Top, Pixels(50.0), move |n, ctx| {
            div()
                // .flex()
                .bg(rgb(0x2e7d32))
                .size_12()
                // .justify_center()
                // .items_center()
                .text_xl()
                .text_color(rgb(0xffffff))
                .child(format!("Hello, {n}!"))
                .into_any_element()
        });
        list(ls)
    }
}

fn main() {
    App::new().run(|cx: &mut AppContext| {
        cx.open_window(WindowOptions::default(), |cx| {
            cx.new_view(|_cx| HelloWorld {
                text: "kWorld".into(),
            })
        });
    });
}
