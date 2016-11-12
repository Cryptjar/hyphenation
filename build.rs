extern crate bincode;
extern crate pocket_resources;
extern crate serde;
extern crate unicode_normalization;

extern crate klpattern;

use std::env;
use std::error;
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::io::prelude::*;
use std::path::Path;

use serde::ser;
use bincode::SizeLimit;
use bincode::serde as bin;

use klpattern::{KLPair, KLPTrie, Exceptions, Patterns};


// User configuration

use configurable::*;

mod configurable {
    use unicode_normalization::*;
    use std::str::Chars;

    // In service of configurable normalization forms, a type alias and a function
    // are defined via conditional compilation.
    //
    // If no feature is explicitly set, we default to the declarations for NFC.
    
    // Neither Cargo nor rustc allows us to set exclusive features; we must indulge
    // them with this clumsy branle of cfg declarations.

    #[cfg(any(any(feature = "nfc", feature = "nfkc"),
              not(any(feature = "nfc", feature = "nfd",
                      feature = "nfkc", feature = "nfkd",
                      feature = "none"))))]
    pub type Normalizer<'a> = Recompositions<Chars<'a>>;

    #[cfg(any(feature = "nfd", feature = "nfkd"))]
    pub type Normalizer<'a> = Decompositions<Chars<'a>>;

    #[cfg(feature = "none")]
    pub type Normalizer<'a> = Chars<'a>;


    #[cfg(any(feature = "nfc",
              not(any(feature = "nfc", feature = "nfd",
                      feature = "nfkc", feature = "nfkd",
                      feature = "none"))))]
    pub fn normalize<'a>(s: &'a str) -> Normalizer<'a> { s.nfc() }

    #[cfg(feature = "nfd")]  pub fn normalize<'a>(s: &'a str) -> Normalizer<'a> { s.nfd() }
    #[cfg(feature = "nfkc")] pub fn normalize<'a>(s: &'a str) -> Normalizer<'a> { s.nfkc() }
    #[cfg(feature = "nfkd")] pub fn normalize<'a>(s: &'a str) -> Normalizer<'a> { s.nfkd() }
    #[cfg(feature = "none")] pub fn normalize<'a>(s: &'a str) -> Normalizer<'a> { s.chars() }
}


// Pattern parsing

trait KLPattern {
    fn klp(String) -> Self;
    fn unklp(&self) -> &str;

    fn value(char) -> Option<u8>;

    fn non_scoring(c: &char) -> bool {
        Self::value(c.clone()) == None
    }

    fn parse_score<I>(cs: I) -> Vec<u8> where I: Iterator<Item = char> {
        let mut result = vec![];
        let mut skip_interval = false;

        for c in cs {
            match (Self::value(c), skip_interval) {
                (Some(n), _) => {
                    skip_interval = true;
                    result.push(n);
                },
                (None, false) => result.push(0),
                (None, true) => skip_interval = false
            }
        }

        if !skip_interval { result.push(0) }
        result
    }

    fn klpair(&self) -> KLPair {
        let str_klp = &self.unklp();
        let normalized: String = normalize(str_klp).collect();
        let as_chars = normalized.chars();

        let alphabetical: String = as_chars.clone().filter(Self::non_scoring).collect();
        let score = Self::parse_score(as_chars);

        (alphabetical, score)
    }
}


#[derive(Clone, Debug)]
struct Pattern(String);

#[derive(Clone, Debug)]
struct Exception(String);

impl KLPattern for Pattern {
    fn klp(s: String) -> Self { Pattern(s) }
    fn unklp(&self) -> &str { self.0.as_ref() }

    fn value(c: char) -> Option<u8> { c.to_digit(10).map(|n| n as u8) }
}

impl KLPattern for Exception {
    fn klp(s: String) -> Self { Exception(s) }
    fn unklp(&self) -> &str { self.0.as_ref() }

    fn value(c: char) -> Option<u8> {
        match c == '-' {
            true => Some(1),
            false => None
        }
    }
}


// Pattern IO and serialization

pub fn source_klp_file(lang: &str, suffix: &str) -> Result<File, Error> {
    let _wdir = &env::var("CARGO_MANIFEST_DIR") ?;
    let work_dir = Path::new(_wdir);
    let fname = format!("hyph-{}.{}.txt", lang, suffix);
    let fpath = work_dir.join("patterns-tex").join(fname);

    Ok( File::open(fpath) ? )
}

pub fn load_by_line(lang: &str, suffix: &str) -> Result<io::Lines<io::BufReader<File>>, Error> {
    let file = source_klp_file(lang, suffix) ?;
    let reader = io::BufReader::new(file);

    Ok(reader.lines())
}

trait KLPTrieIO<'a> : KLPTrie<'a> + ser::Serialize {
    type KLP : KLPattern;

    fn suffix_in() -> &'static str;
    fn suffix_out() -> &'static str;

    fn build(mut self, lang: &str) -> Self where Self: Sized {
        let textual_klps = load_by_line(lang, Self::suffix_in()).unwrap();
        let mut klpairs: Vec<KLPair> = textual_klps.map(|p| Self::KLP::klp(p.unwrap()).klpair()).collect();

        klpairs.sort();
        klpairs.dedup();

        for klp in klpairs.into_iter() { self.insert(klp); }

        self
    }

    fn write(&self, lang: &'a str) -> Result<&'a str, Error> {
        let str_workdir = &env::var("CARGO_MANIFEST_DIR") ?;
        let work_dir = Path::new(str_workdir);
        let fname = format!("{}.{}.bincode", lang, Self::suffix_out());
        let fpath = work_dir.join("patterns").join(fname);

        let mut buffer = io::BufWriter::new( File::create(fpath) ? );

        bin::serialize_into(&mut buffer, &self, SizeLimit::Bounded(10_000_000)) ?;
        buffer.write("\n".as_bytes()) ?;

        Ok(lang)
    }
}

impl<'a> KLPTrieIO<'a> for Patterns {
    type KLP = Pattern;

    fn suffix_in() -> &'static str { "pat" }
    fn suffix_out() -> &'static str { "patterns" }
}

impl<'a> KLPTrieIO<'a> for Exceptions {
    type KLP = Exception;

    fn suffix_in() -> &'static str { "hyp" }
    fn suffix_out() -> &'static str { "exceptions" }
}


fn main() {
    let output_suffixes = vec![Patterns::suffix_out(), Exceptions::suffix_out()];
    let langs = vec![
        "af",
        "hy",
        "as",
        "eu",
        "bn",
        "bg",
        "ca",
        "zh-latn-pinyin",
        "cop",
        "hr",
        "cs",
        "da",
        "nl",
        "en-gb",
        "en-us",
        "eo",
        "et",
        "mul-ethi",
        "fi",
        "fr",
        "fur",
        "gl",
        "ka",
        "de-1901",
        "de-1996",
        "de-ch-1901",
        "grc",
        "el-monoton",
        "el-polyton",
        "gu",
        "hi",
        "hu",
        "is",
        "id",
        "ia",
        "ga",
        "it",
        "kn",
        "kmr",
        "la",
        "la-x-classic",
        "lv",
        "lt",
        "ml",
        "mr",
        "mn-cyrl",
        "nb",
        "nn",
        "oc",
        "or",
        "pa",
        "pms",
        "pl",
        "pt",
        "ro",
        "rm",
        "ru",
        "sa",
        "sr-cyrl",
        "sh-cyrl",
        "sh-latn",
        "cu",
        "sk",
        "sl",
        "es",
        "sv",
        "ta",
        "te",
        "th",
        "tr",
        "tk",
        "uk",
        "hsb",
        "cy"
    ];


    for lang in langs.iter() {
        let patterns = Patterns::new().build(lang);
        let exceptions = Exceptions::new().build(lang);

        fs::create_dir_all("patterns").unwrap();
        patterns.write(lang).unwrap();
        exceptions.write(lang).unwrap();
    }

    let resource_paths =
        langs.iter().flat_map(|tag|
                        output_suffixes.iter().map(move |suffix|
                            ("patterns", format!("{}.{}.bincode", tag, suffix))));

    pocket_resources::package(resource_paths.collect::<Vec<_>>().iter()).unwrap();
}


// Error type boilerplate

#[derive(Debug)]
pub enum Error {
    Env(env::VarError),
    IO(io::Error),
    Serialization(bin::SerializeError),
    Resource
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Env(ref e) => e.description(),
            Error::IO(ref e) => e.description(),
            Error::Serialization(ref e) => e.description(),
            Error::Resource => "Pattern resource creation failed"
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Env(ref e) => e.fmt(f),
            Error::IO(ref e) => e.fmt(f),
            Error::Serialization(ref e) => e.fmt(f),
            Error::Resource => {
                let e = self as &error::Error;
                e.description().fmt(f)
            }
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IO(err)
    }
}

impl From<env::VarError> for Error {
    fn from(err: env::VarError) -> Error {
        Error::Env(err)
    }
}

impl From<bin::SerializeError> for Error {
    fn from(err: bin::SerializeError) -> Error {
        Error::Serialization(err)
    }
}
