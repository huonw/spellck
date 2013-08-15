# spellck

A spell-checker for Rust code. Examines each `pub` item and their
documentation for spelling errors, in a very naive way (just comparing
the words with a dictionary, not even attempting stemming, by default
just `/usr/share/dict/words`).

It breaks idents like `foo_bar` and `FooBar` into `foo` and `bar`, and
ignores any numbers/non-alphanumeric characters.

Compile with `rustc -O main.rs` (in theory it works with `rustpkg`
too), and run with `./main path/to/file.rs`.

Known to work with rust commit
790e6bb3972c3b167a8e0314305740a20f62d2f0.

## Args

- `-d`, `--dict`: supply an extra dictionary, one word per line (can be listed multiple times)
- `-n`, `--no-def-dict`: don't load `/usr/share/dict/words` by default
