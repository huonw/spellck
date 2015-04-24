#![crate_name = "spellck"]
#![feature(plugin_registrar, rustc_private)]

extern crate syntax;
#[macro_use] extern crate rustc;

extern crate stem;

use rustc::plugin::Registry;

pub mod words;
pub mod visitor;

mod lint;

#[plugin_registrar]
pub fn plugin_registrar(reg: &mut Registry) {
    reg.register_lint_pass(Box::new(lint::Misspellings::load()));
}
