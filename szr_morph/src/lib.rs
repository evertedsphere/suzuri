#![allow(dead_code)]
mod blob;
mod dart;
mod hasher;
mod io;
mod pathing;
mod unkchar;
mod userdict;

use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Read},
    ops::{Deref, Range},
    str,
};

use snafu::Snafu;
use tracing::{error, trace};

pub use crate::{
    blob::Blob,
    userdict::{RawUserDict, UserDict},
};
use crate::{dart::*, io::*, unkchar::*};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    IoError {
        source: std::io::Error,
    },
    /// FIXME remove this
    #[snafu(whatever, display("{message}: {source:?}"))]
    CatchallError {
        message: String,
        #[snafu(source(from(Box<dyn std::error::Error>, Some)))]
        source: Option<Box<dyn std::error::Error>>,
    },
}

#[derive(Clone, Debug)]
pub struct FormatToken {
    left_context: u16,
    right_context: u16,

    pos: u16,
    cost: i64,

    original_id: u32,

    feature_offset: u32,
}

impl FormatToken {
    fn read<T: Read + std::io::Seek>(sysdic: &mut T, original_id: u32) -> Result<FormatToken> {
        let ret = FormatToken {
            left_context: read_u16(sysdic)?,
            right_context: read_u16(sysdic)?,
            pos: read_u16(sysdic)?,
            cost: read_i16(sysdic)? as i64,
            original_id,
            feature_offset: read_u32(sysdic)?,
        };

        // seek away a u32 of padding
        seek_rel_4(sysdic)?;

        Ok(ret)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenType {
    /// Token came from a mecab dictionary.
    Normal,
    /// Token came from a user dictionary.
    User,
    /// Token over section of text not covered by dictionary (unknown).
    UNK,
    /// Used internally for virtual beginning-of-string and end-of-string
    /// tokens. Not exposed to outside functions.
    BOS,
}

#[derive(Clone, Debug)]
pub struct LexerToken {
    /// Used internally during lattice pathfinding.
    left_context: u16,
    /// Used internally during lattice pathfinding.
    right_context: u16,

    /// I don't know what this is.
    pos: u16,
    /// Used internally during lattice pathfinding.
    pub cost: i64,
    /// Cost updated to include right-edge connection cost after parsing.
    pub real_cost: i64,

    /// The range, in bytes, to which this token corresponds to in the original
    /// text.
    pub range: Range<usize>,

    /// Origin of token. BOS and UNK are virtual origins
    /// ("beginning/ending-of-string" and "unknown", respectively). Normal means
    /// it came from the mecab dictionary.
    ///
    /// The BOS (beginning/ending-of-string) tokens are stripped away in
    /// parse_to_lexertokens.
    pub kind: TokenType,

    /// Unique identifier of what specific lexeme realization this is, from the
    /// mecab dictionary. changes between dictionary versions.
    pub original_id: u32,

    pub feature_offset: u32,
}

impl LexerToken {
    /// Returns the text to which this token corresponds to in the original
    /// text.
    ///
    /// The `whole_text` is the original string for which you've
    /// called [`Dict::tokenise`] or [`Dict::tokenise_with_cache`].
    pub fn get_text<'a>(&self, whole_text: &'a str) -> &'a str {
        &whole_text[self.range.clone()]
    }

    /// Returns a feature string corresponding to this token.
    ///
    /// Feature strings are dictionary-specific so unfortunately
    /// you need to parse them yourself. They usually contain
    /// things like the exact part-of-speech this token represents,
    /// its reading, whenever it's conjugated or not, etc.
    pub fn get_feature<'a>(&self, dict: &'a Dict) -> Option<&'a str> {
        dict.read_feature_string(self)
    }
}

struct EdgeInfo {
    blob: Blob,
}

impl EdgeInfo {
    fn new(blob: Blob) -> EdgeInfo {
        EdgeInfo { blob }
    }
}

/// A cache for internal allocations.
pub struct Cache {
    pathing_cache: crate::pathing::Cache,
    tokens: Vec<Token<'static>>,
}

impl Cache {
    pub fn new() -> Self {
        Cache {
            pathing_cache: crate::pathing::Cache::new(),
            tokens: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TokeniseError {
    _dummy: (),
}

impl std::fmt::Display for TokeniseError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "failed to tokenise the input")
    }
}

impl std::error::Error for TokeniseError {}

pub struct Dict {
    sys_dic: DartDict,
    unk_dic: DartDict,
    unk_data: UnkChar,
    user_dic: Option<UserDict>,

    use_space_stripping: bool,
    use_unk_forced_processing: bool,
    use_unk_greedy_grouping: bool,
    use_unk_prefix_grouping: bool,

    left_edges: u16,
    right_edges: u16,

    matrix: EdgeInfo,
}

impl Dict {
    /// Load sys.dic and matrix.bin files into memory and prepare the data
    /// that's stored in them to be used by the parser.
    ///
    /// Returns a Dict or, on error, a string describing an error that prevented
    /// the Dict from being created.
    ///
    /// Only supports UTF-8 mecab dictionaries with a version number of 0x66.
    ///
    /// Ensures that sys.dic and matrix.bin have compatible connection matrix
    /// sizes.
    pub fn load(sysdic: Blob, unkdic: Blob, matrix: Blob, unkchar: Blob) -> Result<Dict> {
        trace!("loading sys.dic");
        let sys_dic = load_mecab_dart_file(sysdic)?;
        trace!("loading unk.dic");
        let unk_dic = load_mecab_dart_file(unkdic)?;
        let unk_data = load_char_bin(&mut Cursor::new(unkchar))?;

        let mut matrix_cursor = Cursor::new(matrix.as_ref());
        let left_edges = read_u16(&mut matrix_cursor)?;
        let right_edges = read_u16(&mut matrix_cursor)?;

        if sys_dic.left_contexts != left_edges as u32
            || sys_dic.right_contexts != right_edges as u32
        {
            panic!("sys.dic and matrix.bin have inconsistent left/right edge counts");
        }

        Ok(Dict {
            sys_dic,
            unk_dic,
            unk_data,
            user_dic: None,
            use_space_stripping: true,
            use_unk_forced_processing: true,
            use_unk_greedy_grouping: true,
            use_unk_prefix_grouping: true,
            left_edges,
            right_edges,

            matrix: EdgeInfo::new(matrix),
        })
    }

    /// Load a user dictionary, comma-separated fields.
    ///
    /// The first four fields are the surface, left context ID, right context
    /// ID, and cost of the token.
    ///
    /// Everything past the fourth comma is treated as pure text and is the
    /// token's feature string. It is itself normally a list of comma-separated
    /// fields with the same format as the feature strings of the main mecab
    /// dictionary.
    pub fn load_user_dictionary(
        &mut self,
        user_dict: Vec<(String, String, FormatToken)>,
    ) -> Result<()> {
        // let mut user_dic_file = Cursor::new(userdic);
        let mut user_dic = UserDict::new();
        // user_dic.load_from(&mut user_dic_file)?;
        user_dic.load_data(user_dict)?;
        self.user_dic = Some(user_dic);
        Ok(())
    }

    /// Returns the feature string belonging to a LexerToken.
    pub fn read_feature_string(&self, token: &LexerToken) -> Option<&str> {
        self.read_feature_string_by_source(token.kind, token.feature_offset)
    }

    /// Calling this with values not taken from a real token is unsupported
    /// behavior.
    pub fn read_feature_string_by_source(&self, kind: TokenType, offset: u32) -> Option<&str> {
        match kind {
            TokenType::UNK => Some(self.unk_dic.feature_get(offset)),
            TokenType::Normal | TokenType::BOS => Some(self.sys_dic.feature_get(offset)),
            TokenType::User => self.user_dic.as_ref().unwrap().feature_get(offset),
        }
    }

    /// Tokenises a string by creating a lattice of possible tokens over it
    /// and finding the lowest-cost path thought that lattice.
    ///
    /// See [`Dict::tokenise_with_cache`] for more details.
    pub fn tokenise(&self, text: &str) -> Result<(Vec<LexerToken>, i64), TokeniseError> {
        let mut cache = Cache::new();
        let mut tokens = Vec::new();
        self.analyse_with_cache(&mut cache, text, &mut tokens)
            .map(|cost| (tokens, cost))
    }

    /// Tokenises a string by creating a lattice of possible tokens over it
    /// and finding the lowest-cost path thought that lattice.
    ///
    /// If successful the contents of `output` will be replaced with a list
    /// of tokens and the total cost of the tokenization will be returned.
    /// If unsuccessful the `output` will be cleared and a `None` will be
    /// returned.
    ///
    /// The dictionary itself defines what tokens exist, how they appear in
    /// the string, their costs, and the costs of their possible connections.
    ///
    /// It's possible for multiple paths to tie for the lowest cost. It's not
    /// defined which path is returned in that case.
    ///
    /// If you'll be calling this method multiple times you should reuse the
    /// same `Cache` object across multiple invocations for increased
    /// efficiency.
    pub fn analyse_with_cache(
        &self,
        cache: &mut Cache,
        text: &str,
        output: &mut Vec<LexerToken>,
    ) -> Result<i64, TokeniseError> {
        if text.is_empty() {
            return Ok(0);
        }

        fn take_memory<'a, 'b>(vec: &mut Vec<Token<'a>>) -> Vec<Token<'b>> {
            vec.clear();
            // This is safe since we cleared the vector, so the inner lifetime doesn't
            // matter.
            let mut vec: &mut Vec<Token<'b>> = unsafe { std::mem::transmute(vec) };
            let mut out = Vec::new();
            std::mem::swap(&mut out, &mut vec);
            out
        }

        let mut tokens = take_memory(&mut cache.tokens);
        generate_potential_tokens(self, text, &mut tokens);

        let (path, total_cost) = crate::pathing::shortest_path(
            &mut cache.pathing_cache,
            tokens.len(),
            |index| tokens[index].rank as u32,
            |index| tokens[index].range.end as u32,
            |left, right| {
                let right_token = &tokens[right];
                let left_token = &tokens[left];
                right_token.cost as i64
                    + self.access_matrix(left_token.right_context, right_token.left_context) as i64
            },
            |index| {
                let right_token = &tokens[index];
                right_token.cost as i64 + self.access_matrix(0, right_token.left_context) as i64
            },
            |index| self.access_matrix(tokens[index].right_context, 0) as i64,
        );

        output.clear();
        output.extend(path.iter().map(|&index| (&tokens[index as usize]).into()));

        for i in 0..output.len() {
            let left_context = if i == 0 {
                0
            } else {
                output[i - 1].right_context
            };
            let right_context = output[i].left_context;
            let edge_cost = self.access_matrix(left_context, right_context);
            output[i].real_cost = output[i].cost + edge_cost as i64;
        }

        cache.tokens = take_memory(&mut tokens);
        if path.is_empty() {
            error!("failed to tokenise text of length {}: {}", text.len(), text);
            return Err(TokeniseError { _dummy: () });
        }

        Ok(total_cost)
    }

    #[allow(clippy::cast_lossless)]
    fn access_matrix(&self, left: u16, right: u16) -> i16 {
        let matrix = &self.matrix;
        let location = self.left_edges as u32 * right as u32 + left as u32;
        // the 4 is for the two u16s at the beginning that specify the shape of the
        // matrix
        let offset = 4 + location as usize * 2;
        let cost = &matrix.blob[offset..offset + 2];
        i16::from_le_bytes([cost[0], cost[1]])
    }

    /// Set whether the 0x20 whitespace stripping behavior is enabled. Returns
    /// the previous value of the setting.
    ///
    /// Enabled by default.
    ///
    /// When enabled, spaces are virtually added to the front of the next
    /// token/tokens during lattice construction. This has the effect of turning
    /// 0x20 whitespace sequences into forced separators without affecting
    /// connection costs, but makes it slightly more difficult to reconstruct
    /// the exact original text from the output of the parser.
    pub fn set_space_stripping(&mut self, setting: bool) {
        self.use_space_stripping = setting;
    }

    /// Set whether support for forced unknown token processing is enabled.
    /// Returns the previous value of the setting.
    ///
    /// Enabled by default.
    ///
    /// When the parser's input string has locations where no entries can be
    /// found in the dictionary, the parser has to fill that location with
    /// unknown tokens. The unknown tokens are made by grouping up as many
    /// compatible characters as possible AND/OR grouping up every group of
    /// compatible characters from a length of 1 to a length of N. Whether
    /// either type of grouping is done (and how long the maximum prefix group
    /// is) is specified for each character in the unknown character data
    /// (usually char.bin).
    ///
    /// The unknown character data can also specify that certain character types
    /// always trigger grouping into unknown tokens, even if the given location
    /// in the input string can be found in a normal dictionary. Disabling this
    /// setting will override that data and cause the lattice builder to ONLY
    /// create unknown tokens when nothing can be found in a normal dictionary.
    ///
    /// If all unknown character processing at some problematic point in the
    /// input string fails for some reason, such as a defective unknown
    /// character data file, or one or both of the grouping modes being
    /// disabled, then that problematic point in the input string will create a
    /// single-character unknown token.
    ///
    /// When enabled, the unknown character data's flag for forcing processing
    /// is observed. When disabled, it is ignored, and processing is never
    /// forced.
    pub fn set_unk_forced_processing(&mut self, setting: bool) {
        self.use_unk_forced_processing = setting;
    }

    /// Set whether greedy grouping behavior is enabled. Returns the previous
    /// value of the setting.
    ///
    /// Enabled by default.
    ///
    /// When enabled, problematic locations in the input string will (if
    /// specified in the unknown character data) be greedily grouped into an
    /// unknown token, covering all compatible characters.
    ///
    /// Note that this does not prevent real words inside of the grouping from
    /// being detected once the lattice constructor comes around to them, which
    /// means that greedy grouping does not necessarily override prefix
    /// grouping, and for some character types, the unknown character data will
    /// have both greedy grouping and prefix grouping enabled.
    pub fn set_unk_greedy_grouping(&mut self, setting: bool) {
        self.use_unk_greedy_grouping = setting;
    }

    /// Set whether greedy grouping behavior is enabled. Returns the previous
    /// value of the setting.
    ///
    /// Enabled by default. See the documentation for the other set_unk_
    /// functions for an explanation of what unknown token prefix grouping is.
    pub fn set_unk_prefix_grouping(&mut self, setting: bool) {
        self.use_unk_prefix_grouping = setting;
    }
}

#[derive(Debug)]
struct Token<'a> {
    rank: u32,
    range: Range<usize>,
    kind: TokenType,
    format_token: &'a FormatToken,
}

impl<'a> Token<'a> {
    fn new(
        format_token: &'a FormatToken,
        rank: usize,
        range: Range<usize>,
        kind: TokenType,
    ) -> Self {
        Token {
            rank: rank as u32,
            range: range.start..range.end,
            kind,
            format_token,
        }
    }
}

impl<'a> Deref for Token<'a> {
    type Target = FormatToken;

    fn deref(&self) -> &Self::Target {
        &self.format_token
    }
}

impl<'a> From<&'a Token<'a>> for LexerToken {
    fn from(token: &'a Token<'a>) -> Self {
        LexerToken {
            left_context: token.left_context,
            right_context: token.right_context,
            pos: token.pos,
            cost: token.cost,
            real_cost: 0,
            range: token.range.clone(),
            kind: token.kind,
            original_id: token.original_id,
            feature_offset: token.feature_offset,
        }
    }
}

fn generate_potential_tokens_at<'a>(
    dict: &'a Dict,
    text: &str,
    mut start: usize,
    output: &mut Vec<Token<'a>>,
) -> usize {
    let initial_output_len = output.len();
    let rank = start;

    let space_count;
    if dict.use_space_stripping {
        space_count = text[start..]
            .bytes()
            .take_while(|&byte| byte == b' ')
            .count();
        start += space_count;
    } else {
        space_count = 0;
    }

    let mut index_iter = text[start..].char_indices();
    let mut end = start;
    let first_char = if let Some((_, c)) = index_iter.next() {
        end += c.len_utf8();
        c
    } else {
        return space_count;
    };

    // find all tokens starting at this point in the string
    let mut hasher = crate::hasher::Hasher::new();
    hasher.write_u32(first_char as u32);
    loop {
        let substring: &str = &text[start..end];
        let hash = hasher.finish();
        let mut any = false;
        if dict.sys_dic.may_contain(hash) {
            any = true;
            if let Some(matching_tokens) = dict.sys_dic.dic_get(&substring) {
                let tokens = matching_tokens
                    .into_iter()
                    .map(|token| Token::new(token, rank, start..end, TokenType::Normal));
                output.extend(tokens);
            }
        }
        if dict
            .user_dic
            .as_ref()
            .map(|x| x.may_contain(substring))
            .unwrap_or(false)
        {
            any = true;
            if let Some(matching_tokens) = dict
                .user_dic
                .as_ref()
                .and_then(|user_dic| user_dic.dic_get(&substring))
            {
                let tokens = matching_tokens
                    .into_iter()
                    .map(|token| Token::new(token, rank, start..end, TokenType::User));
                output.extend(tokens);
            }
        }

        if !any {
            break;
        }

        if let Some((_, c)) = index_iter.next() {
            hasher.write_u32(c as u32);
            end += c.len_utf8();
        } else {
            break;
        }
    }

    // build unknown tokens if appropriate
    let start_type = &dict.unk_data.get_type(first_char);

    if (dict.use_unk_greedy_grouping || dict.use_unk_prefix_grouping)
        && ((dict.use_unk_forced_processing && dict.unk_data.always_process(first_char))
            || output.len() == initial_output_len)
    {
        let mut unk_end = start;

        let do_greedy = dict.use_unk_greedy_grouping && start_type.greedy_group;
        let do_prefix = dict.use_unk_prefix_grouping && start_type.prefix_group_len > 0;
        let mut prefix_len = if do_prefix {
            start_type.prefix_group_len
        } else {
            0
        } as usize;

        // find possible split points and furthest allowed ending in advance
        let mut unk_indices = vec![];
        for (_, c) in text[start..].char_indices() {
            if dict.unk_data.has_type(c, start_type.number) {
                unk_end += c.len_utf8();
                unk_indices.push(unk_end);
                // stop building when necessary
                if !do_greedy && unk_indices.len() >= prefix_len {
                    break;
                }
            } else {
                break;
            }
        }
        prefix_len = std::cmp::min(prefix_len, unk_indices.len());

        if let Some(matching_tokens) = dict.unk_dic.dic_get(&start_type.name) {
            for token in matching_tokens {
                if do_greedy {
                    output.push(Token::new(token, rank, start..unk_end, TokenType::UNK));
                }
                for end in unk_indices[0..prefix_len].iter() {
                    output.push(Token::new(token, rank, start..*end, TokenType::UNK));
                }
            }
        }
    }

    let first_char_len = first_char.len_utf8();
    let mut build_unknown_single = |name| {
        if output.len() != initial_output_len {
            return;
        }

        if let Some(default_tokens) = dict.unk_dic.dic_get(name) {
            if let Some(first_token) = default_tokens.iter().next() {
                output.push(Token::new(
                    first_token,
                    rank,
                    start..start + first_char_len,
                    TokenType::UNK,
                ));
            }
        }
    };

    // build fallback token if appropriate
    build_unknown_single(&start_type.name);
    build_unknown_single("DEFAULT");
    if output.len() == initial_output_len {
        panic!("unknown chars dictionary has a broken DEFAULT token");
    }

    space_count
}

fn generate_potential_tokens<'a>(dict: &'a Dict, text: &str, output: &mut Vec<Token<'a>>) {
    let mut skip_until_after = 0;
    for i in 0..=text.len() {
        if i < skip_until_after || !text.is_char_boundary(i) {
            continue;
        }

        let skipnext = generate_potential_tokens_at(dict, text, i, output);
        skip_until_after = i + skipnext;
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    fn assert_implements_sync<T>()
    where
        T: Sync,
    {
    }
    fn assert_implements_send<T>()
    where
        T: Send,
    {
    }

    // concatenate surface forms of parsertoken stream, with given comma between
    // tokens
    fn tokenstream_to_string(input: &str, stream: &Vec<LexerToken>, comma: &str) -> String {
        let mut ret = String::new();

        let mut first = true;
        for token in stream {
            if !first {
                ret += comma;
            }
            ret += token.get_text(input);
            first = false;
        }
        ret
    }

    fn assert_parse(dict: &Dict, input: &str, truth: &str) {
        println!("testing parse...");
        let result = dict.tokenise(input).unwrap();

        for token in &result.0 {
            println!("{}", token.get_feature(dict).expect("no feature"));
        }
        let split_up_string = tokenstream_to_string(input, &result.0, "|");
        println!("{}", split_up_string);

        assert_eq!(split_up_string, truth);
    }

    fn file_to_string(file: &mut File) -> String {
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        text
    }

    #[test]
    fn test_various() {
        assert_implements_sync::<Dict>();
        assert_implements_send::<Dict>();

        // you need to acquire a mecab dictionary and place these files here manually
        // These tests will probably fail if you use a different dictionary than me.
        // That's normal. Different dicionaries parse differently.
        let sysdic = Blob::open("/home/s/c/szr/data/system/unidic-cwj-3.1.0/sys.dic").unwrap();
        let unkdic = Blob::open("/home/s/c/szr/data/system/unidic-cwj-3.1.0/unk.dic").unwrap();
        let matrix = Blob::open("/home/s/c/szr/data/system/unidic-cwj-3.1.0/matrix.bin").unwrap();
        let unkdef = Blob::open("/home/s/c/szr/data/system/unidic-cwj-3.1.0/char.bin").unwrap();

        let mut dict = Dict::load(sysdic, unkdic, matrix, unkdef).unwrap();

        // general nonbrokenness
        assert_parse(&dict, "これ", "これ");

        assert_parse(&dict, "これを", "これ|を");

        assert_parse(&dict, "これを持っていけ", "これ|を|持っ|て|いけ");

        // lots of text
        assert_parse(&dict,
          "メタプログラミング (metaprogramming) とはプログラミング技法の一種で、ロジックを直接コーディングするのではなく、あるパターンをもったロジックを生成する高位ロジックによってプログラミングを行う方法、またその高位ロジックを定義する方法のこと。主に対象言語に埋め込まれたマクロ言語によって行われる。",
          "メタ|プログラミング|(|metaprogramming|)|と|は|プログラミング|技法|の|一種|で|、|ロジック|を|直接|コーディング|する|の|で|は|なく|、|ある|パターン|を|もっ|た|ロジック|を|生成|する|高位|ロジック|に|よっ|て|プログラミング|を|行う|方法|、|また|その|高位|ロジック|を|定義|する|方法|の|こと|。|主に|対象|言語|に|埋め込ま|れ|た|マクロ|言語|に|よっ|て|行わ|れる|。"
        );

        // lorem ipsum
        // This test will CERTAINLY fail if you don't have the same mecab dictionary.
        // updated for 3.1.0, not sure about spoken vs written
        assert_parse(&dict,
          "Lorem sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
          "Lorem|s|i|t|a|m|e|t|,|consectetur|adipiscing|elit|,|sed|do|eiusmod|tempor|incididunt|u|t|labore|e|t|dolore|magna|aliqua|."
        );

        // string that is known to trigger problems with at least one buggy pathfinding
        // algorithm morph used before
        // original version
        // assert_parse(&dict,
        // "だっでおら、こんな、こんなにっ！飛車角のこと、好きなんだでっ！！！！！！",
        // "だっ|で|おら|、|こんな|、|こんな|に|っ|！|飛車|角|の|こと|、|好き|な|ん|だ|で|っ|！|！|！|！|！|！"
        // );
        // version that should be agnostic w/r/t spoken language vs written language
        // variants of unidic 2.3.0
        assert_parse(&dict,
          "だっでおら、こんな、こんなにっ！飛車角のこと、好きなんだ！！！！！！",
          "だっ|で|おら|、|こんな|、|こんな|に|っ|！|飛車|角|の|こと|、|好き|な|ん|だ|！|！|！|！|！|！"
        );

        // unknown character token stuff
        assert_parse(&dict, "噛", "噛");
        assert_parse(&dict, "噛 ", "噛");
        assert_parse(&dict, "噛\n", "噛|\n");

        // overrides
        dict.set_space_stripping(false);
        assert_parse(&dict, "a b", "a| |b");
        dict.set_space_stripping(true);
        assert_parse(&dict, "」   ", "」");

        assert_parse(&dict, "噛噛", "噛噛");
        dict.set_unk_prefix_grouping(false);
        dict.set_unk_greedy_grouping(false);
        assert_parse(&dict, "噛噛", "噛|噛");
        dict.set_unk_prefix_grouping(true);
        dict.set_unk_greedy_grouping(true);

        assert_parse(&dict, "programmprogram", "programmprogram");
        dict.set_unk_forced_processing(false);
        assert_parse(&dict, "programmprogram", "program|m|program");
        dict.set_unk_forced_processing(true);

        // hentaigana
        assert_parse(&dict, "𛁁", "𛁁");

        // user dictionary
        // assert_parse(&dict, "飛行機", "飛行|機");
        // dict.load_user_dictionary().unwrap();
        // assert_parse(&dict, "飛行機", "飛行機");
    }
}
