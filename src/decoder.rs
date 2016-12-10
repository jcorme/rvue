use std::collections::HashMap;
use std::num::{ParseFloatError, ParseIntError};
use std::str::ParseBoolError;

use chrono::{self, NaiveDate};
use xml::attribute::OwnedAttribute;
use xml::reader::{Error as ReaderError, Events, XmlEvent as ReaderEvent};

pub trait SVUEDecodeable {
    fn from_event(event: ReaderEvent, events_iter: &mut Events<&[u8]>)
        -> DecoderResult<Self> where Self: Sized;
}

pub type DecoderResult<T> = Result<T, DecodingError>;

#[derive(Debug)]
pub enum DecodingError {
    BoolParseError(String, ParseBoolError),
    DateParseError(String, chrono::ParseError),
    SVUEErrorParsingFailed(String),
    EventError(ReaderError),
    FloatParseError(String, ParseFloatError),
    IntegerParseError(String, ParseIntError),
    MissingAttribute(String),
    UnexpectedEnd,
    UnexpectedEvent(ReaderEvent),
}

#[inline]
pub fn parse_date(date: &str) -> Result<NaiveDate, chrono::ParseError>  {
    NaiveDate::parse_from_str(date, "%-m/%-d/%Y")
}

#[inline]
pub fn attributes_vec_to_map<'a>(attrs: &'a Vec<OwnedAttribute>) -> HashMap<&'a str, String> {
    attrs.iter()
        .map(|a| (a.name.local_name.as_str(), a.value.clone()))
        .fold(HashMap::new(), |mut acc, (k, v)| { acc.insert(k, v); acc })
}

#[macro_export]
macro_rules! get_attr {
    ( $attrs:expr, $attr:expr ) => {
        match $attrs.get($attr) {
            Some(val) => val,
            None => { return Err(DecodingError::MissingAttribute($attr.into())); }
        }
    };
}

#[macro_export]
macro_rules! get_attr_owned {
    ( $attrs:expr, $attr:expr ) => {
        get_attr!($attrs, $attr).clone()
    };
}

#[macro_export]
macro_rules! parse_date {
    ( $attrs:expr, $attr:expr ) => {
        {
            let date = parse_date(get_attr!($attrs, $attr));

            if date.is_err() {
                return Err(DecodingError::DateParseError($attr.into(), date.unwrap_err()));
            }

            date.unwrap()
        }
    };
}

#[macro_export]
macro_rules! parse_bool {
    ( $attrs:expr, $attr:expr ) => {
        {
            let b = bool::from_str(get_attr!($attrs, $attr));

            if b.is_err() {
                return Err(DecodingError::BoolParseError($attr.into(), b.unwrap_err()));
            }

            b.unwrap()
        }
    };
}

#[macro_export]
macro_rules! parse_int {
    ( $ity:tt, $attrs:expr, $attr:expr ) => {
        {
            let int = $ity::from_str_radix(get_attr!($attrs, $attr), 10);

            if int.is_err() {
                return Err(DecodingError::IntegerParseError($attr.into(), int.unwrap_err()));
            }

            int.unwrap()
        }
    };
}

#[macro_export]
macro_rules! parse_float {
    ( $fty:tt, $attrs:expr, $attr:expr ) => {
        {
            let f = $fty::from_str(get_attr!($attrs, $attr));

            if f.is_err() {
                return Err(DecodingError::FloatParseError($attr.into(), f.unwrap_err()));
            }

            f.unwrap()
        }
    };
}
