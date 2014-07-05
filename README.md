# spellck

[![Build Status](https://travis-ci.org/huonw/spellck.png)](https://travis-ci.org/huonw/spellck)

A public API spell-checker plugin for the Rust compiler. Examines most
`pub` things like `mod`s, `fn`s, `struct`s and their fields, `enum`s
and their variants, as well as their documentation for spelling
errors.  It acts in a very naive way: just comparing the words with a
dictionary.

It breaks idents like `foo_bar` and `FooBar` into `foo` and `bar`, and
ignores any numbers/non-alphanumeric characters.

The dictionary format is just a listing of words, one per
line. `src/stdlib.txt` is the words/abbreviations/sequences of letters
from the Rust standard library that are correct, but are not in my
`/usr/share/dict/words`.

Known to work with Rust commit 90ab2f8.

## Installation

This is Cargo-enabled, and can be used as a normal cargo dependency.

## Lint

The compiler plugin is simply used by loading the crate as a plugin.

Any uses of the plugin must have the `SPELLCK_LINT_DICT` environment
variable specified, pointing at the dictionary files to be used
(multiple can be specified, in the same format as the platform's
`PATH` variable). This is required because, unfortunately, compiler
plugins cannot take any arguments yet.

```toml
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
```

```
$ SPELLCK_LINT_DICT=/usr/share/dict/words cargo build
SPELLCK_LINT_DICT=/usr/share/dict/words cargo build
       Fresh spellck v0.2.0 (https://github.com/huonw/spellck)
   Compiling spellck_example v0.0.0 (file:...)
spellck_example.rs:6:1: 6:19 warning: misspelled words: dok, coment, #[warn(misspellings)] on by default
spellck_example.rs:6 /// Bad dok coment
                     ^~~~~~~~~~~~~~~~~~
spellck_example.rs:7:1: 7:22 warning: misspelled word: mispelled, #[warn(misspellings)] on by default
spellck_example.rs:7 pub fn mispelled() {}
                     ^~~~~~~~~~~~~~~~~~~~~
```

## Standalone

The standalone binary `spellck_standalone` does not require loading a
compiler plugin, it is used like `spellck_standalone
crate_file.rs`. This defaults to using `/usr/share/dict/words` as the
dictionary.

### Args

- `-d`, `--dict`: supply an extra dictionary, one word per line (can
  be listed multiple times)
- `-n`, `--no-def-dict`: don't load `/usr/share/dict/words` by default

## Known bugs/desirable features

- The printing of a span is very naive: just the first line, and so
  for `/** ... */` doc-comments it normally prints just `/**`.
- `pub use mispelled = foo::bar::baz;` is not warned about.
- Could perform stemming.
- Could offer suggestions, e.g. [Norvig's basic corrector](http://norvig.com/spell-correct.html)
