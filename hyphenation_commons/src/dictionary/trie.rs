use fst::Map;
use fst::raw;
use serde::ser::{Serialize, Serializer};
use serde::de::{self, Visitor, Deserialize, Deserializer};

use std::convert::From;
use std::error;
use std::fmt;
use std::slice;


#[derive(Debug, Default)]
pub struct Trie(Map);

impl Trie {
    pub fn as_bytes(&self) -> &[u8] { self.0.as_fst().as_bytes() }

    pub fn from_bytes(bs : Vec<u8>) -> Result<Self, Error> {
        let map = Map::from_bytes(bs) ?;
        Ok(Trie(map))
    }

    pub fn from_iter<I>(iter : I) -> Result<Self, Error>
    where I : Iterator<Item = (String, u64)> {
        let m = fst::Map::from_iter(iter) ?;
        Ok(Trie(m))
    }

    pub fn get_prefixes<'f, 'q>(&'f self, query : &'q [u8]) -> PrefixMatches<'f, 'q> {
        let fst = self.0.as_fst();
        PrefixMatches {
            fst,
            node : fst.root(),
            output : raw::Output::zero(),
            query : query.into_iter()
        }
    }
}

#[derive(Clone)]
pub struct PrefixMatches<'f, 'q> {
    fst : &'f raw::Fst,
    node : raw::Node<'f>,
    output : raw::Output,
    query : slice::Iter<'q, u8>,
}

impl<'f, 'q> Iterator for PrefixMatches<'f, 'q> {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        let prefix = &mut self.query;
        for &b in prefix {
            match self.node.find_input(b) {
                None => return None,
                Some(i) => {
                    let t = self.node.transition(i);
                    self.output = self.output.cat(t.out);
                    self.node = self.fst.node(t.addr);
                    if self.node.is_final() {
                        let final_output = self.output.cat(self.node.final_output());
                        return Some(final_output.value())
                    }
                }
            }
        }

        None
    }
}


impl AsRef<fst::Map> for Trie {
    fn as_ref(&self) -> &fst::Map { &self.0 }
}

impl AsMut<fst::Map> for Trie {
    fn as_mut(&mut self) -> &mut fst::Map { &mut self.0 }
}

impl From<fst::Map> for Trie {
    fn from(m : fst::Map) -> Self { Trie(m) }
}


/*
Serialization
*/

#[derive(Copy, Clone, Debug)]
struct FstVisitor;

const NOM_DE_SER : &'static str = "Trie";

impl<'de> Visitor<'de> for FstVisitor {
    type Value = Trie;

    fn expecting(&self, f : &mut fmt::Formatter) -> fmt::Result {
        f.write_str("the internal trie of a hyphenation dictionary")
    }

    fn visit_bytes<E>(self, bs : &[u8]) -> Result<Self::Value, E>
    where E : de::Error {
        Trie::from_bytes(bs.to_vec()).map_err(E::custom)
    }

    fn visit_byte_buf<E>(self, bs : Vec<u8>) -> Result<Self::Value, E>
    where E : de::Error {
        Trie::from_bytes(bs).map_err(E::custom)
    }

    fn visit_newtype_struct<D>(self, de : D) -> Result<Self::Value, D::Error>
    where D : Deserializer<'de> {
        de.deserialize_bytes(FstVisitor)
    }
}

impl Serialize for Trie {
    fn serialize<S>(&self, ser : S) -> Result<S::Ok, S::Error>
    where S : Serializer {
        ser.serialize_newtype_struct(NOM_DE_SER, self.as_bytes())
    }
}

impl<'de> Deserialize<'de> for Trie {
    fn deserialize<D>(de : D) -> Result<Self, D::Error>
    where D : Deserializer<'de> {
        de.deserialize_newtype_struct(NOM_DE_SER, FstVisitor)
    }
}


#[derive(Debug)]
pub enum Error {
    Fst(fst::Error),
    Serialization(String)
}

impl fmt::Display for Error {
    fn fmt(&self, f : &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Error::Serialization(ref s) => format!("Dictionary serialization failed: {}", s),
            Error::Fst(ref err) =>
                format!("The dictionary's internal trie could not be built:\n{}", err)
        };

        f.write_str(&message)
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Error::Fst(ref err) => Some(err),
            _ => None
        }
    }
}

impl From<fst::Error> for Error {
    fn from(err : fst::Error) -> Self {
        Error::Fst(err)
    }
}
