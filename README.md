# spellck

[![Build Status](https://travis-ci.org/huonw/spellck.png)](https://travis-ci.org/huonw/spellck)

A public API spell-checker plugin for the `rustc` Rust compiler. It
finds spelling errors in the names and documentation of most exported
things like `mod`s, `fn`s, `struct`s and their fields, `enum`s and
their variants.

Identifiers like `foo_bar` and `FooBar` are broken into `foo` and
`bar`, with numbers/non-alphabetic characters acting as separators. It
acts in a very naive way: just comparing the words with a dictionary.

The dictionary format is just a listing of words, one per
line. `src/stdlib.txt` is the words/abbreviations/sequences of letters
from the Rust standard library that are correct, but are not in my
`/usr/share/dict/words`.

Known to work with Rust commit e09d98603 2014-11-18 23:51:43.


## Installation

This is Cargo-enabled, and can be used as a normal Cargo
dependency. Both the library/compiler plugin and the standalone
executable are built with `cargo build`.

## Lint

The checker comes as compiler plugin that enables it to run during the
normal compilation process, it is used by simply loading the crate as
a plugin. This causes the compiler to emit warnings (by default) for
incorrect words via the `misspellings` lint; one can use
`#[deny(misspellings)]` to make mistakes errors, and
`#[allow(misspellings)]` to stop the warnings, like with other lints.

```toml
# Cargo.toml
[package]
name = "spellck_example"
version = "0.0.0"
authors = []

[dependencies.spellck]
git = "https://github.com/huonw/spellck"

[[bin]]
name = "spellck_example"
path = "spellck_example.rs"
crate_type = ["lib"]
```

```rust
// spellck_example.rs
#![feature(phase)]

#[phase(plugin)] extern crate spellck;

/// Bad dok coment
pub fn mispelled() {}

#[allow(misspellings)]
/// This is public, but poor speeling won't get a warning.
pub struct Notaword {
    pub madeuptext: uint
}
```

Any uses of the plugin must have the `SPELLCK_LINT_DICT` environment
variable specified, pointing at the dictionary files to be used
(compiler plugins cannot take any arguments yet). Multiple can be
specified, in the same format as the platform's `PATH` variable.

```
$ SPELLCK_LINT_DICT=/usr/share/dict/words cargo build
       Fresh spellck v0.2.0 (https://github.com/huonw/spellck)
   Compiling spellck_example v0.0.0 (file:...)
spellck_example.rs:6:1: 6:19 warning: misspelled words: dok, coment, #[warn(misspellings)] on by default
spellck_example.rs:6 /// Bad dok coment
                     ^~~~~~~~~~~~~~~~~~
spellck_example.rs:7:1: 7:22 warning: misspelled word: mispelled, #[warn(misspellings)] on by default
spellck_example.rs:7 pub fn mispelled() {}
                     ^~~~~~~~~~~~~~~~~~~~~
```

Words can also be added to the list of valid words using an attribute containing a string of space separated words in the crate root like so:

```rust
// lib.rs -- the crate root
#![spellck_extra_words="these are not in word list but are now"]

#[phase(plugin)] extern crate spellck;
```

At the moment, the explicit `extern crate` is required as there is no
other way to load plugins
([#15446](https://github.com/rust-lang/rust/issues/15446)). The
`extern crate` declaration could have a `#[cfg(check_spelling)]`
attribute, so that the lint is only loaded and run when compiled with
`--cfg check_spelling` is specified.


## Standalone

The standalone binary `spellck_standalone` does not require loading a
compiler plugin, it is used like `spellck_standalone
crate_file.rs`. This defaults to using `/usr/share/dict/words` as the
dictionary. (A default `cargo build` will output the resulting binary
as `target/spellck_standalone`.)

### Args

- `-d`, `--dict`: supply an extra dictionary, one word per line (can
  be listed multiple times)
- `-n`, `--no-def-dict`: don't load `/usr/share/dict/words` by default
