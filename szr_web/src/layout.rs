use szr_html::{Doc, Render, Z};

pub fn is_punctuation(s: &str) -> bool {
    s.chars().count() == 1
        && matches!(
            s.chars().next(),
            Some(
                '「' | '」'
                    | '。'
                    | '、'
                    | '？'
                    | '！'
                    | '　'
                    | '─'
                    | '）'
                    | '（'
                    | '…'
                    | '︙'
                    | '《'
                    | '》'
            )
        )
}

pub fn labelled_value_c<'a, V: Render, W: Render>(
    label: W,
    value: V,
    classes: &'static str,
) -> Doc {
    Z.div()
        .class("flex flex-row gap-4 items-baseline")
        .c(Z.span()
            .class("font-bold text-gray-600 shrink-0 whitespace-nowrap")
            .c(label))
        .c(Z.div().class(classes).c(value))
}

pub fn labelled_value<W: Render, V: Render>(label: V, value: W) -> Doc {
    labelled_value_c(label, value, "")
}

pub fn head() -> Doc {
    let fonts_preamble = (
        Z.link()
            .rel("preconnect")
            .href("https://fonts.googleapis.com"),
        Z.link()
            .rel("preconnect")
            .href("https://fonts.gstatic.com")
            .crossorigin(""),
        Z.stylesheet("https://fonts.googleapis.com/css2?family=Sawarabi+Gothic&display=swap"),
    );
    let tailwind_preamble = Z.stylesheet("/static/output.css");
    let icons_preamble = Z.stylesheet("https://unpkg.com/boxicons@2.1.4/css/boxicons.min.css");
    let htmx_preamble = Z.script().src("/static/htmx.min.js");

    Z.head()
        .c(htmx_preamble)
        .c(fonts_preamble)
        .c(tailwind_preamble)
        .c(icons_preamble)
}
