use std::{
    collections::BTreeMap,
    fs::File,
    io::BufReader,
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
use crate::assert_golden_json;
use libepub::{archive::EpubArchive, doc::EpubDoc};
use regex::Regex;
use serde::Serialize;
use sha2::Digest;
pub use snafu::prelude::*;
use tl::{HTMLTag, Node, Parser};
use tracing::{error, instrument, trace, warn};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Ctx)))]
pub enum Error {
    UnsupportedFormat {
        err: FormatError,
    },
    ParseError {
        source: tl::ParseError,
    },
    CreateEpubDocError {
        source: libepub::doc::DocError,
    },
    CreateEpubArchiveError {
        source: libepub::archive::ArchiveError,
    },
}

#[derive(Debug)]
pub enum FormatError {
    NoTitle,
    NoHtmlForPage,
}

#[derive(Debug, Clone, Serialize)]
pub enum Element {
    Line(String),
    Image(String),
}

#[derive(Debug, Serialize)]
pub struct Book {
    pub title: String,
    pub chapters: Vec<Chapter>,
    #[serde(skip)]
    pub images: BTreeMap<PathBuf, Vec<u8>>,
    pub images_hash: String,
}

#[derive(Debug, Serialize)]
pub struct Chapter {
    pub title: String,
    pub len: usize,
    pub start_pos: usize,
    pub lines: Vec<Element>,
}

#[test]
fn read_input_files() {
    let input_files = glob::glob("input/*.epub").unwrap().collect::<Vec<_>>();
    for f in input_files {
        let f = f.unwrap();
        println!("file: {:?}", f);
        let r = parse(&f);
        assert!(r.is_ok());
        if let Ok(r) = r {
            assert_golden_json!(r.title, r);
        }
    }
}

#[instrument]
pub fn parse(path: &std::path::Path) -> Result<Book> {
    let mut doc = EpubDoc::new(path).context(CreateEpubDocCtx)?;
    let mut archive = EpubArchive::new(path).context(CreateEpubArchiveCtx)?;

    let num_pages = doc.get_num_pages();

    // epubs are required to have titles
    let title = doc.mdata("title").context(UnsupportedFormatCtx {
        err: FormatError::NoTitle,
    })?;

    let has_toc = doc.toc.len() > 0;
    let has_nav = doc.resources.get("nav").is_some();

    // TODO clean up title, strip author/pub names inserted in there etc
    // TODO grab more info from book metadata
    trace!(title, num_pages, has_toc, has_nav, "read book");

    let raw_chapter_hrefs: Vec<(String, String)> = if has_toc {
        doc.toc
            .iter()
            .filter_map(|n| {
                let lbl = n.label.clone();
                if let Some(content) = n.content.to_str() {
                    Some((lbl, content.to_owned()))
                } else {
                    error!(lbl, "failed to convert content to string!");
                    None
                }
            })
            .collect::<Vec<_>>()
    } else if has_nav {
        let (nav_path, _nav_mime_type) = doc.resources.get("nav").cloned().unwrap();
        // trace!("{:?}", nav_path);
        let nav_content = doc.get_resource_str_by_path(nav_path).unwrap();

        let dom = tl::parse(&nav_content, tl::ParserOptions::default()).context(ParseCtx)?;

        let parser = dom.parser();
        // ideally nav > ol > li > a
        dom.query_selector("a")
            .unwrap()
            .map(|li| {
                let tag = li.get(parser).unwrap().as_tag().unwrap();
                let href = tag
                    .attributes()
                    .get("href")
                    .unwrap()
                    .unwrap()
                    .as_utf8_str()
                    .into_owned();
                let title = tag.inner_text(parser).into_owned();
                (title, href)
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new() // TODO extend by 0
    };

    // ignore anchors for now
    // TODO chunk up to the anchor on the page?

    let re = Regex::new("#").unwrap();

    // (title, start page number, id of start element)
    let chapter_markers: Vec<(String, usize, Option<String>)> = raw_chapter_hrefs
        .into_iter()
        .map(|(title, href)| {
            let mut uri = re.split(&href);
            let mut p = PathBuf::new();
            p.push(uri.next().unwrap().to_string());
            let page_number = doc.resource_uri_to_chapter(&p).unwrap();
            let start_id = uri.next().map(str::to_string);
            (title, page_number, start_id)
        })
        .collect::<Vec<_>>();

    trace!("{:?}", chapter_markers);

    // FIXME assert >0 chapter markers

    // in an APL you could probably chunk the whole thing by chapter indices

    let mut chapters = Vec::<Chapter>::new();

    let mut chars_so_far = 0;

    let page_count = doc.get_num_pages();
    let mut images = BTreeMap::new();

    for (chapter_num, chapter_marker) in chapter_markers.iter().enumerate() {
        // FIXME dev
        // if chapter_num > 2 {
        //     break;
        // }

        let mut chapter_chars;
        let mut chapter_lines;

        let chapter_start_page = chapter_marker.1;
        // do we have content before the start of the first chapter?
        // やっかいだな
        // should be able to do current_page == 0 here, right?
        if chapter_num == 0 && doc.get_current_page() < chapter_start_page {
            warn!("garbage before first chapter, attempting to recover");
            (chapter_chars, chapter_lines) =
                get_chapter_lines(&mut doc, &mut archive, &mut images, chapter_start_page - 1)?;
            chars_so_far += chapter_chars;

            chapters.push(Chapter {
                title: "garbage before first chapter".to_owned(),
                len: chapter_chars,
                start_pos: chars_so_far,
                lines: chapter_lines,
            });
        }

        let chapter_end_page = if chapter_num < chapter_markers.len() - 1 {
            chapter_markers[chapter_num + 1].1
        } else {
            page_count
        };

        (chapter_chars, chapter_lines) =
            get_chapter_lines(&mut doc, &mut archive, &mut images, chapter_end_page - 1)?;

        let title = chapter_marker.0.clone();

        chapters.push(Chapter {
            title: title.clone(),
            len: chapter_chars,
            start_pos: chars_so_far,
            lines: chapter_lines.clone(),
        });

        chars_so_far += chapter_chars;
    }

    let mut hasher = sha2::Sha256::new();
    for (k, v) in images.iter() {
        hasher.update(k.to_str().unwrap());
        hasher.update(v);
    }
    let hash = hasher.finalize();

    Ok(Book {
        title,
        chapters,
        images,
        images_hash: format!("{:x}", hash),
    })
}

fn get_chapter_lines(
    doc: &mut EpubDoc<BufReader<File>>,
    archive: &mut EpubArchive<BufReader<File>>,
    images: &mut BTreeMap<PathBuf, Vec<u8>>,
    stop_on_page: usize,
) -> Result<(usize, Vec<Element>)> {
    let mut len = 0;
    let mut lines = Vec::new();

    while doc.get_current_page() < stop_on_page {
        doc.go_next(); // this will consume the nth page!
        let (page_len, mut page_lines) = get_page_lines(doc, archive, images)?;
        len += page_len;
        lines.extend(page_lines.drain(..));
    }

    Ok((len, lines))
}

//
//
//
//
//
//
//
//
//
// TODO parallelise
//
//
//
//
//
//
//
//
//
//

fn get_page_lines(
    doc: &mut EpubDoc<BufReader<File>>,
    archive: &mut EpubArchive<BufReader<File>>,
    images: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(usize, Vec<Element>)> {
    let s = doc
        .get_current_str()
        .context(UnsupportedFormatCtx {
            err: FormatError::NoHtmlForPage,
        })?
        .0;

    let mut len = 0;
    let dom = tl::parse(&s, tl::ParserOptions::default()).context(ParseCtx)?;
    let r = dom
        .nodes()
        .iter()
        .filter_map(|n| {
            let parser = dom.parser();
            n.as_tag().and_then(|tag| {
                let (text_len, lines) = get_tag_lines(doc, archive, images, parser, tag)?;
                len += text_len;
                Some(lines)
            })
        })
        .collect::<Vec<_>>();
    Ok((len, r))
}

fn get_tag_lines(
    doc: &mut EpubDoc<BufReader<File>>,
    archive: &mut EpubArchive<BufReader<File>>,
    images: &mut BTreeMap<PathBuf, Vec<u8>>,
    parser: &Parser,
    tag: &HTMLTag,
) -> Option<(usize, Element)> {
    // trace!("{}", tag.name());
    if tag.name() == "p" {
        // even images are usually within <p> tags
        // so we take the text and not the html
        // TODO maybe someday run a bit of ocr on gaiji and
        // replace with actual unicode
        let inner = collect_text(&tag, &parser);
        if inner.len() > 0 {
            let len = count_ja_chars(&inner);
            return Some((len, Element::Line(inner)));
        }
    } else if tag.name() == "img" || tag.name() == "image" {
        let get_attr = |attr| {
            tag.attributes()
                .get(attr)
                .flatten()
                .map(|x| x.as_utf8_str().to_string())
        };
        if let Some(rel_uri) = get_attr("src")
            .or(get_attr("href"))
            .or(get_attr("xlink:href"))
        {
            trace!("found image uri {}", rel_uri);
            let page_dir = doc.get_current_path()?.parent()?.to_owned();
            let uri = normalize_path(&page_dir.join(&rel_uri));
            trace!("normalized: {:?}", uri);
            let contents = archive.get_entry(&uri).ok()?;
            images.insert(uri.clone(), contents);
            trace!("inserting uri {:?}", uri);
            return Some((0, Element::Image(uri.to_str().unwrap().to_owned())));
        } else {
            warn!("image node without source");
        }
    }
    None
}

fn count_ja_chars(s: &str) -> usize {
    // stolen from ttu:
    // https://github.com/ttu-ttu/ebook-reader/blob/39411145562daf12d7ea7b9300525ae40c022b60/apps/web/src/lib/functions/get-character-count.ts#L14
    let re = Regex::new(
        r"[0-9A-Z○◯々-〇〻ぁ-ゖゝ-ゞァ-ヺー０-９Ａ-Ｚｦ-ﾝ\p{Radical}\p{Unified_Ideograph}]+",
    )
    .unwrap();
    re.find_iter(s)
        .map(|c| c.as_str().chars().count())
        .sum::<usize>()
}

fn collect_text(tag: &HTMLTag, parser: &Parser) -> String {
    tag.children()
        .top()
        .iter()
        .filter_map(|child| match child.get(parser).unwrap() {
            Node::Raw(raw) => Some(raw.as_utf8_str().to_string()),
            Node::Tag(child) => {
                match child.name().as_utf8_str().into_owned().as_str() {
                    "span" | "a" => Some(child.inner_text(parser).into_owned()),
                    "ruby" => {
                        Some(
                            child
                                .children()
                                .top()
                                .iter()
                                .filter_map(|ruby_child| {
                                    match ruby_child.get(parser).unwrap() {
                                        Node::Raw(raw) => Some(raw.as_utf8_str()),
                                        Node::Tag(rb) => {
                                            // TODO in future if the API grows to support
                                            // ruby hints, this is where you'd add that
                                            if rb.name() == "rb" {
                                                Some(rb.inner_text(parser))
                                            } else {
                                                // TODO check rt and err if not that either
                                                None
                                            }
                                        }
                                        Node::Comment(_) => None,
                                    }
                                })
                                .collect::<String>(),
                        )
                    }
                    _ => None,
                }
            }
            Node::Comment(_) => None,
        })
        .collect::<String>()
}

// https://github.com/rust-lang/cargo/blob/fede83ccf973457de319ba6fa0e36ead454d2e20/src/cargo/util/paths.rs#L61
// h/t matklad here https://old.reddit.com/r/rust/comments/hkkquy/anyone_knows_how_to_fscanonicalize_but_without/fwtw53s/
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}
