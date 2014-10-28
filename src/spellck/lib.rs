#![crate_name = "spellck"]
#![feature(phase, plugin_registrar)]


extern crate syntax;
#[phase(plugin, link)] extern crate rustc;

extern crate stem;

use rustc::plugin::Registry;

pub mod words;
pub mod visitor;

mod lint;

#[plugin_registrar]
pub fn plugin_registrar(reg: &mut Registry) {
    reg.register_lint_pass(box lint::Misspellings::load());
}
