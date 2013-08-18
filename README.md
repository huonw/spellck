# spellck

A spell-checker for Rust code. Examines all<sup>1</sup> `pub` things
like `mod`s, `fn`s, `struct`s and their fields, `enum`s and their
variants, as well as their documentation for spelling errors.  It acts
in a very naive way: just comparing the words with a dictionary, not
even attempting stemming; by default just `/usr/share/dict/words`. It
*doesn't* offer suggestions.

It breaks idents like `foo_bar` and `FooBar` into `foo` and `bar`, and
ignores any numbers/non-alphanumeric characters.

Compile with `rustc -O main.rs` (in theory it works with `rustpkg`
too), and run with `./main path/to/crate.rs`.

`self.txt` and `stdlib.txt` are the words/abbreviations/sequences of
letters from `spellck` and `std` & `extra` respectively that are
correct, but are not in my `/usr/share/dict/words`.

Known to work with Rust commit 6a88415.

<sup>1</sup> Not guaranteed; Rust's privacy rules are non-obvious, and are
currently buggy anyway.

## Args

- `-d`, `--dict`: supply an extra dictionary, one word per line (can
  be listed multiple times)
- `-n`, `--no-def-dict`: don't load `/usr/share/dict/words` by default

## Known bugs

- The printing of a span is very naive: just the first line, and so
  for `/** ... */` doc-comments it normally prints just `/**`.
- `/*! ... */` doc-comments attached to a module get the incorrect
  span, so they print the next item, not the start of the doc-comment
  itself.  (This is possibly a more general problem with `/*! ... */`,
  I haven't investigated yet.)
