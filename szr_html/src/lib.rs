use std::{borrow::Cow, fmt, fmt::Arguments, io};

use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
};

impl IntoResponse for Doc {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::OK, Html(self.render_to_string())).into_response()
    }
}

/// Rendering logic responsible for string escaping and such.
///
/// See `Renderer` for implementation.
pub trait Renderer {
    /// Normal write: perform escaping etc. if necessary
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.write_raw(data)
    }
    /// Normal write but with `format_args!`
    fn write_fmt(&mut self, fmt: &Arguments) -> io::Result<()> {
        self.write(format!("{}", fmt).as_bytes())
    }
    /// Normal write for `&str`
    fn write_str(&mut self, s: &str) -> io::Result<()> {
        self.write(s.as_bytes())
    }

    /// Raw write: no escaping should be performed
    fn write_raw(&mut self, data: &[u8]) -> io::Result<()>;

    /// Raw write but with `format_args!`
    fn write_raw_fmt(&mut self, fmt: &Arguments) -> io::Result<()> {
        self.write_raw(format!("{}", fmt).as_bytes())
    }

    /// Raw write for `&str`
    fn write_raw_str(&mut self, s: &str) -> io::Result<()> {
        self.write_raw(s.as_bytes())
    }
}

/// A `Renderer` that does not escape anything it renders
///
/// A `Renderer` that uses underlying Renderer to call
/// only `raw` methods, and thus avoid escaping values.
pub struct RawRenderer<'a, T: 'a + ?Sized>(&'a mut T);

impl<'a, T: 'a + Renderer + ?Sized> Renderer for RawRenderer<'a, T> {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.0.write_raw(data)
    }

    fn write_fmt(&mut self, fmt: &Arguments) -> io::Result<()> {
        self.0.write_raw_fmt(fmt)
    }

    fn write_str(&mut self, s: &str) -> io::Result<()> {
        self.0.write_raw_str(s)
    }

    fn write_raw(&mut self, data: &[u8]) -> io::Result<()> {
        self.0.write_raw(data)
    }

    fn write_raw_fmt(&mut self, fmt: &Arguments) -> io::Result<()> {
        self.0.write_raw_fmt(fmt)
    }

    fn write_raw_str(&mut self, s: &str) -> io::Result<()> {
        self.0.write_raw_str(s)
    }
}

/// A value that can be rendered - part or a whole template
///
/// This can be generally thought as a part or a whole `template`,
/// with "the blanks" already filled with a data, but not yet
/// rendered to `Renderer`.
///
/// It is defined for bunch of `std` types. Please send PR if
/// something is missing.
///
/// You can impl it for your own types too. You usually compose it
/// from many other `impl Render` data.
pub trait Render {
    fn render(&self, renderer: &mut dyn Renderer) -> io::Result<()>;
}

// {{{ impl Render
impl<T: Render> Render for Vec<T> {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        for t in self.iter() {
            t.render(r)?;
        }
        Ok(())
    }
}

impl<T: Render> Render for [T] {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        for t in self.iter() {
            t.render(r)?;
        }
        Ok(())
    }
}

// macro_rules! impl_narr {
//     ($n:expr) => {
//         impl<T: Render> Render for [T; $n] {
//             fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
//                 for t in self.iter() {
//                     t.render(r)?;
//                 }
//                 Ok(())
//             }
//         }
//     };
// }

// impl_narr!(0);
// impl_narr!(1);
// impl_narr!(2);
// impl_narr!(3);
// impl_narr!(4);
// impl_narr!(5);
// impl_narr!(6);
// impl_narr!(7);
// impl_narr!(8);
// impl_narr!(9);
// impl_narr!(10);
// impl_narr!(11);
// impl_narr!(12);
// impl_narr!(13);
// impl_narr!(14);
// impl_narr!(15);
// impl_narr!(16);
// impl_narr!(17);
// impl_narr!(18);
// impl_narr!(19);
// impl_narr!(20);
// impl_narr!(21);
// impl_narr!(22);
// impl_narr!(23);
// impl_narr!(24);
// impl_narr!(25);
// impl_narr!(26);
// impl_narr!(27);
// impl_narr!(28);
// impl_narr!(29);
// impl_narr!(30);
// impl_narr!(31);
// impl_narr!(32);

impl<'a, T: Render + ?Sized> Render for &'a mut T {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        (**self).render(r)?;
        Ok(())
    }
}

impl<T: Render + ?Sized> Render for Box<T> {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        (**self).render(r)?;
        Ok(())
    }
}

impl Render for () {
    fn render(&self, _: &mut dyn Renderer) -> io::Result<()> {
        Ok(())
    }
}

impl<R: Render> Render for Option<R> {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        if let &Some(ref s) = self {
            s.render(r)?
        }
        Ok(())
    }
}

impl Render for char {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        String::from(*self).render(r)
    }
}

impl Render for String {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        r.write_raw(self.as_bytes())
    }
}

macro_rules! impl_render_raw {
    ($t:ty) => {
        impl Render for $t {
            fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
                r.write_raw_fmt(&format_args!("{}", self))
            }
        }
    };
}

impl_render_raw!(f64);
impl_render_raw!(f32);
impl_render_raw!(i64);
impl_render_raw!(u64);
impl_render_raw!(i32);
impl_render_raw!(u32);
impl_render_raw!(usize);
impl_render_raw!(isize);

impl<'a> Render for &'a str {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        r.write_str(self)
    }
}

impl<'a> Render for fmt::Arguments<'a> {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        r.write_fmt(self)
    }
}

impl<'a> Render for &'a fmt::Arguments<'a> {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        r.write_fmt(self)
    }
}

impl<A> Render for (A,)
where
    A: Render,
{
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        self.0.render(r)
    }
}

impl<A, B> Render for (A, B)
where
    A: Render,
    B: Render,
{
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        self.0.render(r)?;
        self.1.render(r)
    }
}

impl<A, B, C> Render for (A, B, C)
where
    A: Render,
    B: Render,
    C: Render,
{
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        self.0.render(r)?;
        self.1.render(r)?;
        self.2.render(r)
    }
}

impl<A, B, C, D> Render for (A, B, C, D)
where
    A: Render,
    B: Render,
    C: Render,
    D: Render,
{
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        self.0.render(r)?;
        self.1.render(r)?;
        self.2.render(r)?;
        self.3.render(r)
    }
}
impl<A, B, C, D, E> Render for (A, B, C, D, E)
where
    A: Render,
    B: Render,
    C: Render,
    D: Render,
    E: Render,
{
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        self.0.render(r)?;
        self.1.render(r)?;
        self.2.render(r)?;
        self.3.render(r)?;
        self.4.render(r)
    }
}

impl<A, B, C, D, E, F> Render for (A, B, C, D, E, F)
where
    A: Render,
    B: Render,
    C: Render,
    D: Render,
    E: Render,
    F: Render,
{
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        self.0.render(r)?;
        self.1.render(r)?;
        self.2.render(r)?;
        self.3.render(r)?;
        self.4.render(r)?;
        self.5.render(r)
    }
}

impl<A, B, C, D, E, F, G> Render for (A, B, C, D, E, F, G)
where
    A: Render,
    B: Render,
    C: Render,
    D: Render,
    E: Render,
    F: Render,
    G: Render,
{
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        self.0.render(r)?;
        self.1.render(r)?;
        self.2.render(r)?;
        self.3.render(r)?;
        self.4.render(r)?;
        self.5.render(r)?;
        self.6.render(r)
    }
}

impl<A, B, C, D, E, F, G, H> Render for (A, B, C, D, E, F, G, H)
where
    A: Render,
    B: Render,
    C: Render,
    D: Render,
    E: Render,
    F: Render,
    G: Render,
    H: Render,
{
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        self.0.render(r)?;
        self.1.render(r)?;
        self.2.render(r)?;
        self.3.render(r)?;
        self.4.render(r)?;
        self.5.render(r)?;
        self.6.render(r)?;
        self.7.render(r)
    }
}

/// Use to wrap closures with
pub struct Fn<F>(pub F);

impl<F> Render for Fn<F>
where
    F: std::ops::Fn(&mut dyn Renderer) -> io::Result<()>,
{
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        self.0(r)
    }
}

pub trait RenderExt: Render {
    fn render_to_vec(&self) -> Vec<u8> {
        let mut v: Vec<u8> = vec![];
        self.render(&mut v).unwrap();
        v
    }

    fn render_to_string(&self) -> String {
        String::from_utf8_lossy(&self.render_to_vec()).into()
    }
}

impl<T: Render + ?Sized> RenderExt for T {}

impl<T: io::Write> Renderer for T {
    fn write_raw(&mut self, data: &[u8]) -> io::Result<()> {
        self.write_all(data)
    }

    fn write_raw_fmt(&mut self, fmt: &fmt::Arguments) -> io::Result<()> {
        self.write_fmt(*fmt)
    }

    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        for c in data {
            match *c as char {
                '&' => self.write_all(b"&amp;")?,
                '<' => self.write_all(b"&lt;")?,
                '>' => self.write_all(b"&gt;")?,
                '"' => self.write_all(b"&quot;")?,
                '\'' => self.write_all(b"&#x27;")?,
                '/' => self.write_all(b"&#x2F;")?,
                _ => self.write_all(&[*c])?,
            }
        }

        Ok(())
    }
}

#[allow(unused)]
pub fn raw<T: Render>(x: T) -> impl Render {
    Fn(move |r: &mut dyn Renderer| x.render(&mut RawRenderer(r)))
}

////

macro_rules! for_each {
    ($f:ident; ) => {};
    ($f:ident; $t:ident $(, $ts:ident)*) => {
        $f!($t, stringify!($t));
        for_each!($f; $($ts),*);
    };
}

//////////////////////////////////////////////////////////////////////////////////////////

pub struct Z;

macro_rules! impl_tag {
    ($t:ident, $n:expr) => {
        #[allow(unused)]
        pub fn $t(self) -> Doc {
            // this should be moved into the macro
            let tag_name = $n.replace("_", "-");
            self.tag(&tag_name)
        }
    };
}

// TODO make doc able to have a single hole in it while the pre- and post- are rendered
// so you can do skeleton.c().... etc and have it go into r
// i guess it could be like a pointer to the right point in the tree...
//
// Z.div()
//     .c(Z.span()
//         .c(Z.div().c(Z.span().c(dict).cs(defs.0, |s| Z.p().c(s)))))
// could become
// Z.div().c_span().c_div().c()
impl Z {
    for_each!(impl_tag;
              html, div, script, link, meta, head, body,
              h1, h2, h3, h4, h5, h6,
              table, tr, td, th,
              button,
              hr, br, span, a, p, ruby, rt, ul, ol, li);

    pub fn doctype(self, t: &'static str) -> impl Render {
        Fn(move |r: &mut dyn Renderer| {
            r.write_raw(b"<!DOCTYPE ")?;
            r.write_raw_str(t)?;
            r.write_raw(b">")
        })
    }

    pub fn fragment(self) -> Doc {
        Doc {
            tag: None,
            attrs: vec![],
            inn: String::from(""),
        }
    }

    pub fn tag(self, t: &str) -> Doc {
        Doc {
            // see impl_tag! for complaints
            tag: Some(Cow::from(String::from(t))),
            attrs: vec![],
            inn: String::from(""),
        }
    }

    pub fn stylesheet(self, t: &'static str) -> Doc {
        self.link().rel("stylesheet").href(t)
    }
}

//////////////////////////////////////////////////////////////////////////////////////////

type CowStr = Cow<'static, str>;

#[derive(Debug)]
pub struct Doc {
    tag: Option<CowStr>,
    attrs: Vec<(CowStr, Option<CowStr>)>,
    inn: String,
}

///

// macro_rules! impl_custom_attr {
//     ($t:ident, $n:expr, $val:expr) => {
//         pub fn $t(self) -> Doc {
//             self.attr(stringify!($n), $val)
//         }
//     };
// }

macro_rules! impl_attr {
    ($t:ident, $n:expr) => {
        #[allow(unused)]
        pub fn $t<V: Into<CowStr>>(self, val: V) -> Doc {
            // this should be moved into the macro
            let tag_name = $n.replace("_", "-");
            self.attr(tag_name, val)
        }
    };
}

macro_rules! impl_flag {
    ($t:ident, $n:expr) => {
        #[allow(unused)]
        pub fn $t(self) -> Doc {
            // this should be moved into the macro
            let tag_name = $n.replace("_", "-").replace("_raw", "");
            self.flag(tag_name)
        }
    };
}

impl Doc {
    for_each!(impl_attr;
              id, class, src, href, rel, lang, name, charset, content,
              up_target, up_cache, up_method, up_interval);

    for_each!(impl_flag;
              up_preload, up_instant, up_poll, up_nav);

    pub fn attr<K: Into<CowStr>, V: Into<CowStr>>(self, key: K, val: V) -> Self {
        let Doc {
            tag,
            mut attrs,
            inn,
        } = self;
        attrs.push((key.into(), Some(val.into())));
        Doc { tag, attrs, inn }
    }

    // not doing this because we don't account for multiple classes :)
    // we just add attrs one by one
    // impl_attr!(flex_row, class, "flex flex-row");

    pub fn flag<K: Into<CowStr>>(self, key: K) -> Self {
        let Doc {
            tag,
            mut attrs,
            inn,
        } = self;
        attrs.push((key.into(), None));
        Doc { tag, attrs, inn }
    }

    pub fn c<T: Render>(self, val: T) -> Self {
        let Doc {
            tag,
            attrs,
            mut inn,
        } = self;
        inn += &val.render_to_string();
        Doc { tag, attrs, inn }
    }

    pub fn cv<T: Render>(self, val: Vec<T>) -> Self {
        self.c(val)
    }

    pub fn cs<A, F, T: Render>(self, val: Vec<A>, f: F) -> Self
    where
        F: FnMut(A) -> T,
    {
        self.c(val.into_iter().map(f).collect::<Vec<_>>())
    }
}

impl Render for Doc {
    fn render(&self, r: &mut dyn Renderer) -> io::Result<()> {
        if let Some(ref tag) = self.tag {
            r.write_raw_str("<")?;
            r.write_raw_str(tag)?;
            for &(ref k, ref v) in self.attrs.iter() {
                r.write_raw_str(" ")?;
                r.write_raw_str(&*k)?;
                if let Some(ref v) = *v {
                    r.write_raw_str("=\"")?;
                    r.write_raw_str(&*v)?;
                    r.write_raw_str("\"")?;
                }
            }

            r.write_raw_str(">")?;
            r.write_raw_str(&*self.inn)?;
            r.write_raw_str("</")?;
            r.write_raw_str(tag)?;
            r.write_raw_str(">")?;
        } else {
            r.write_raw_str(&*self.inn)?;
        }
        Ok(())
    }
}

/////////////

#[test]
fn test_raw_vs_convenience() {
    let l = Z.tag("div").attr("class", "container");
    let r = Z.tag("div").class("container");

    assert_eq!(l.render_to_string(), r.render_to_string());
}
