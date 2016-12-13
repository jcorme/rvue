#![cfg_attr(feature="serde-serialize", feature(proc_macro))]

extern crate chrono;
extern crate regex;
extern crate reqwest;
#[cfg(feature="serde-serialize")]
#[macro_use] extern crate serde_derive;
extern crate xml;

#[macro_use]
mod decoder;
mod api;
mod diff;
mod gradebook;
